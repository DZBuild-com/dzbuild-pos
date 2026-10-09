import { render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "@/app/App";
import { AppRoutes } from "@/app/routes";
import type { BackupStatus, Session, SupplierHit } from "@/lib/ipc/types";
import {
  appInfoFixture,
  backupFixture,
  cashierSession,
  companyFixture,
  ownerSession,
  ownerTile,
  packFixture,
  settingsValues,
} from "@/test/fixtures";
import { cashSessionFixture } from "@/test/fixtures-m1";
import { countFixture, dashboardFixture, supplierFixture } from "@/test/fixtures-m2";
import { activeStatus } from "@/test/fixtures-m9";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";

/** A role that keeps the supplier files and pays them, but does not see the purchases. */
const supplierKeeperSession: Session = {
  ...ownerSession,
  isOwner: false,
  permissions: ["suppliers.manage"],
};

const supplierHit: SupplierHit = {
  id: supplierFixture.id,
  code: supplierFixture.code,
  name: supplierFixture.name,
  phone: supplierFixture.phone,
  wilaya: supplierFixture.wilaya,
  balance: supplierFixture.balance,
  archived: false,
};

/** Whoever sells lands on the sale screen; its closed-till page leads to the back office. */
async function toBackOffice(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: "Retour au menu" }));
}

function mockApp(extra: Record<string, Handler> = {}) {
  return mockCommands({
    app_startup_error: () => null,
    app_startup_notices: () => [],
    // M9: the licence (a lifetime licence: no banner, the till sells)
    license_status: () => activeStatus,
    setup_status: () => ({ completed: true, hasOwner: true, terminalCode: "C1", loginLang: "fr" }),
    auth_session: () => null,
    auth_list_users: () => [ownerTile],
    company_get: () => companyFixture,
    settings_get: ({ key }) => settingsValues[String(key)],
    settings_set: ({ value }) => value,
    datapack_get: () => packFixture,
    cash_current: () => null,
    hardware_get: () => ({
      version: 1,
      printers: [],
      receiptPrinterId: null,
      receipt: {
        printByDefault: true,
        openDrawerOnCash: true,
        drawerBeforePrint: false,
        logo: true,
        secondLang: null,
      },
      scanner: { layout: "us" },
      documents: { defaultPrinter: null, kinds: [], marginsMm: 12 },
    }),
    ...extra,
  });
}

describe("App", () => {
  it("shows the fatal screen with the data folder", async () => {
    const calls = mockApp({
      app_startup_error: () => ({
        error: { code: "database_corrupt" },
        dataDir: "C:\\Users\\shop\\AppData\\Roaming\\dev.dzbuild.pos",
      }),
      app_open_folder: () => null,
    });
    render(<App />);
    expect(await screen.findByText("DZBuild POS ne peut pas ouvrir ses données")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Le fichier qui contient les données de la boutique est endommagé",
    );
    expect(screen.getByText(/dev\.dzbuild\.pos/)).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: /Ouvrir le dossier des données/ }));
    await waitFor(() => expect(calls).toContain("app_open_folder"));
    expect(calls).not.toContain("setup_status");
  });

  it("shows the fatal screen without a folder to open when Windows gave no data folder", async () => {
    mockApp({ app_startup_error: () => ({ error: { code: "io_error" }, dataDir: "" }) });
    render(<App />);
    expect(await screen.findByText("DZBuild POS ne peut pas ouvrir ses données")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Ouvrir le dossier des données/ })).toBeNull();
  });

  it("tells the user when a scheduled restore failed", async () => {
    mockApp({ app_startup_notices: () => [{ kind: "restoreFailed", reason: "backup_invalid" }] });
    const user = userEvent.setup();
    render(<App />);
    const notice = await screen.findByRole("alert");
    expect(notice).toHaveTextContent("La sauvegarde n'a pas pu être restaurée");
    expect(notice).toHaveTextContent("Cette sauvegarde est endommagée et inutilisable.");
    await user.click(within(notice).getByRole("button", { name: "Fermer" }));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(await screen.findByText("Qui travaille ?")).toBeInTheDocument();
  });

  it("starts the setup wizard on first launch", async () => {
    mockApp({
      setup_status: () => ({ completed: false, hasOwner: false, terminalCode: "C1", loginLang: "fr" }),
    });
    render(<App />);
    expect(await screen.findByText("Choisissez la langue du logiciel")).toBeInTheDocument();
  });

  it("shows the login screen when nobody is logged in", async () => {
    mockApp();
    render(<App />);
    expect(await screen.findByText("Qui travaille ?")).toBeInTheDocument();
    expect(await screen.findByText("Supérette El Amel")).toBeInTheDocument();
  });

  it("lands a cashier on the sale screen, and the shell shows only their modules", async () => {
    mockApp({ auth_session: () => cashierSession });
    const user = userEvent.setup();
    render(<App />);
    // After every sign-in (an idle lock too), the till, never the "coming soon" dashboard.
    expect(await screen.findByText("La caisse est fermée")).toBeInTheDocument();
    await toBackOffice(user);
    const nav = await screen.findByRole("navigation", { name: "Menu principal" });
    expect(within(nav).getByRole("link", { name: "Vendre" })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: "Réglages" })).toBeInTheDocument();
    expect(within(nav).queryByRole("link", { name: "Achats" })).toBeNull();
    // `credit.view` runs the customers reports (`ReportGroup::permission()`), not the dashboard
    expect(within(nav).getByRole("link", { name: "Rapports" })).toBeInTheDocument();
    expect(within(nav).queryByRole("link", { name: "Tableau de bord" })).toBeNull();
    expect(screen.queryByText("Tableau de bord : bientôt disponible")).toBeNull();
    expect(screen.getByText("Poste C1")).toBeInTheDocument();
  });

  it("lands a user who does not sell but sees the sales reports on the dashboard", async () => {
    mockApp({
      auth_session: () => ({
        ...ownerSession,
        permissions: ownerSession.permissions.filter((p) => p !== "sales.create"),
      }),
      reports_dashboard: () => dashboardFixture,
      reports_catalog: () => [],
    });
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Tableau de bord" })).toBeInTheDocument();
    expect(await screen.findByText("Ventes du jour")).toBeInTheDocument();
    expect(screen.queryByText("Tableau de bord : bientôt disponible")).toBeNull();
  });

  it("lands a user who neither sells nor sees the sales reports on the first module they may open", async () => {
    mockApp({
      auth_session: () => ({
        ...ownerSession,
        permissions: ownerSession.permissions.filter((p) => p !== "sales.create" && p !== "reports.sales"),
      }),
      documents_list: () => [],
    });
    render(<App />);
    const nav = await screen.findByRole("navigation", { name: "Menu principal" });
    expect(await screen.findByRole("heading", { name: "Tickets" })).toBeInTheDocument();
    expect(within(nav).queryByRole("link", { name: "Tableau de bord" })).toBeNull();
    expect(within(nav).getByRole("link", { name: "Rapports" })).toBeInTheDocument();
  });

  // `ReportGroup::permission()`: the purchases reports need `purchases.view`, the customers ones
  // `credit.view`, not a `reports.*` permission.
  it.each([
    {
      permission: "purchases.view",
      report: {
        kind: "purchases_by_supplier",
        group: "purchases",
        title: "Achats par fournisseur",
        groupTitle: "Achats",
      },
    },
    {
      permission: "credit.view",
      report: {
        kind: "customer_balances",
        group: "customers",
        title: "Soldes clients",
        groupTitle: "Clients",
      },
    },
  ] as const)("opens the reports to a role holding $permission alone", async ({ permission, report }) => {
    const session: Session = { ...ownerSession, isOwner: false, permissions: [permission] };
    mockApp({
      auth_session: () => session,
      reports_catalog: () => [{ ...report, filters: [], dated: false, requiredFilter: null, allowed: true }],
    });
    renderWithProviders(<AppRoutes session={session} />, { route: "/reports" });
    const nav = await screen.findByRole("navigation", { name: "Menu principal" });
    expect(within(nav).getByRole("link", { name: "Rapports" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { level: 1, name: "Rapports" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { level: 2, name: report.groupTitle })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: report.title })).toHaveAttribute(
      "href",
      `/reports/${report.kind}`,
    );
  });

  it("opens the purchases on the supplier directory to whoever keeps the suppliers only", async () => {
    // jsdom has no layout: the virtual table shows rows once its scroller measures something.
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
    mockApp({ auth_session: () => supplierKeeperSession, suppliers_search: () => [supplierHit] });
    render(<App />);
    const nav = await screen.findByRole("navigation", { name: "Menu principal" });
    expect(within(nav).getByRole("link", { name: "Achats" })).toBeInTheDocument();
    const table = await screen.findByRole("table", { name: "Fournisseurs" });
    expect(await within(table).findByText("Cevital")).toBeInTheDocument();
  });

  it("keeps a purchase order closed to whoever keeps the suppliers only", async () => {
    const calls = mockApp({ auth_session: () => supplierKeeperSession, suppliers_search: () => [] });
    // the orders are purchases (`purchases.view`): the address falls back to the directory
    renderWithProviders(<AppRoutes session={supplierKeeperSession} />, { route: "/purchases/orders/o-1" });
    expect(await screen.findByRole("table", { name: "Fournisseurs" })).toBeInTheDocument();
    expect(calls).not.toContain("purchases_get");
  });

  it("opens the stock counts to whoever only counts", async () => {
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
    const counter: Session = { ...supplierKeeperSession, permissions: ["stock.count"] };
    mockApp({ auth_session: () => counter, counts_list: () => [countFixture] });
    render(<App />);
    const nav = await screen.findByRole("navigation", { name: "Menu principal" });
    expect(within(nav).getByRole("link", { name: "Stock" })).toBeInTheDocument();
    expect(await screen.findByRole("tab", { name: "Inventaires" })).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByRole("table", { name: "Inventaires" })).toBeInTheDocument();
  });

  it("switches to Arabic from the top bar without a reload", async () => {
    mockApp({
      auth_session: () => ownerSession,
      auth_set_language: () => ({ ...ownerSession, lang: "ar" }),
    });
    const user = userEvent.setup();
    render(<App />);
    await toBackOffice(user);
    await user.click(await screen.findByRole("radio", { name: "العربية" }));
    await waitFor(() => expect(document.documentElement).toHaveAttribute("dir", "rtl"));
    expect(await screen.findByRole("navigation", { name: "القائمة الرئيسية" })).toBeInTheDocument();
  });

  it("shows the About page to every user, the folders only to settings.data", async () => {
    mockApp({ auth_session: () => cashierSession, app_info: () => appInfoFixture });
    const user = userEvent.setup();
    render(<App />);
    await toBackOffice(user);
    await user.click(await screen.findByRole("link", { name: "Réglages" }));
    // a cashier's settings open on this PC's licence (M9), the section before About
    await user.click(await screen.findByRole("link", { name: "À propos" }));
    expect(await screen.findByText("0.1.0")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Ouvrir les journaux techniques" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Ouvrir le dossier des données" })).toBeNull();
  });

  it("lets a settings.data holder open the logs folder from About", async () => {
    const calls = mockApp({
      auth_session: () => ownerSession,
      app_info: () => appInfoFixture,
      app_open_folder: () => null,
    });
    const user = userEvent.setup();
    render(<App />);
    await toBackOffice(user);
    await user.click(await screen.findByRole("link", { name: "Réglages" }));
    await user.click(await screen.findByRole("link", { name: "À propos" }));
    await user.click(await screen.findByRole("button", { name: "Ouvrir les journaux techniques" }));
    await waitFor(() => expect(calls).toContain("app_open_folder"));
  });

  it("tells when the technical logs could not go to the data folder", async () => {
    mockApp({
      app_startup_notices: () => [
        { kind: "loggingUnavailable", reason: "io_error", fallbackDir: "C:\\Temp\\dzbuild-pos\\logs" },
      ],
    });
    render(<App />);
    const notice = await screen.findByRole("alert");
    expect(notice).toHaveTextContent("Les journaux techniques ne peuvent pas être écrits");
    expect(within(notice).getByText("C:\\Temp\\dzbuild-pos\\logs")).toHaveAttribute("dir", "ltr");
    expect(notice).toHaveTextContent("Vérifiez l'espace libre sur le disque");
  });

  it("tells when the main PC's copy could not be installed and this PC kept its data", async () => {
    mockApp({ app_startup_notices: () => [{ kind: "lanFailed", reason: "snapshot_invalid" }] });
    render(<App />);
    const notice = await screen.findByRole("alert");
    expect(notice).toHaveTextContent("La copie du poste principal n'a pas pu être installée");
    expect(notice).toHaveTextContent("La copie reçue du poste principal est incomplète ou abîmée.");
    expect(notice).toHaveTextContent("Ce poste garde ses données ; rejoignez à nouveau le poste principal.");
  });

  it("tells a till that could not leave the network that it stays a till, not that a copy failed", async () => {
    mockApp({ app_startup_notices: () => [{ kind: "leaveFailed", reason: "io_error" }] });
    render(<App />);
    const notice = await screen.findByRole("alert");
    expect(notice).toHaveTextContent("Ce poste n'a pas pu quitter le réseau");
    expect(notice).toHaveTextContent("il reste poste de caisse en attendant");
    expect(notice).not.toHaveTextContent("La copie du poste principal n'a pas pu être installée");
  });

  it("lists the receipts history for a cashier and shows the closed till", async () => {
    mockApp({ auth_session: () => cashierSession });
    render(<App />);
    await toBackOffice(userEvent.setup());
    const nav = await screen.findByRole("navigation", { name: "Menu principal" });
    expect(within(nav).getByRole("link", { name: "Tickets" })).toBeInTheDocument();
    expect(await screen.findByRole("link", { name: "Caisse fermée" })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Aucune imprimante de tickets" })).toBeInTheDocument();
  });

  it("offers to print again when a receipt fails after a sale", async () => {
    const calls = mockApp({
      auth_session: () => cashierSession,
      cash_current: () => cashSessionFixture,
      hardware_retry_print: () => null,
    });
    const user = userEvent.setup();
    render(<App />);
    expect(await screen.findByLabelText("Scanner ou rechercher un produit")).toBeInTheDocument();
    await emit("print-failed", { documentId: "d-1", number: "TK2026-C1-000123", code: "printer_offline" });
    expect(await screen.findByText("Le ticket TK2026-C1-000123 n'a pas été imprimé")).toBeInTheDocument();
    expect(screen.getByText(/L'imprimante ne répond pas/)).toBeInTheDocument();
    // The printer dot of the sale screen keeps the receipt until it prints.
    expect(screen.getByRole("button", { name: "Reçus non imprimés : 1" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Réimprimer" }));
    await waitFor(() => expect(calls).toContain("hardware_retry_print"));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Reçus non imprimés : 1" })).toBeNull());
  });

  it("keeps a failed receipt in the printer dot's list, with Réimprimer", async () => {
    const calls = mockApp({
      auth_session: () => cashierSession,
      hardware_retry_print: () => Promise.reject({ code: "print_not_pending" }),
      documents_reprint: () => ({ printer: "Caisse 1", bytes: 900, raster: false }),
    });
    const user = userEvent.setup();
    render(<App />);
    await toBackOffice(user);
    await emit("print-failed", { documentId: "d-7", number: "TK2026-C1-000124", code: "printer_offline" });
    await user.click(await screen.findByRole("button", { name: "Reçus non imprimés : 1" }));
    const sheet = await screen.findByRole("dialog", { name: "Reçus non imprimés" });
    expect(within(sheet).getByText("TK2026-C1-000124")).toBeInTheDocument();
    await user.click(within(sheet).getByRole("button", { name: "Réimprimer" }));
    await waitFor(() => expect(calls).toContain("documents_reprint"));
    expect(await within(sheet).findByText("Tous les reçus ont été imprimés.")).toBeInTheDocument();
  });

  it("falls back to a duplicate when the original was already printed", async () => {
    const calls = mockApp({
      auth_session: () => cashierSession,
      hardware_retry_print: () => Promise.reject({ code: "print_not_pending" }),
      documents_reprint: () => ({ printer: "Caisse 1", bytes: 900, raster: false }),
    });
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("La caisse est fermée");
    await emit("print-failed", { documentId: "d-1", number: "TK2026-C1-000123", code: "printer_timeout" });
    await user.click(await screen.findByRole("button", { name: "Réimprimer" }));
    await waitFor(() => expect(calls).toContain("documents_reprint"));
  });

  it("reprints a versement receipt that already printed as a versement DUPLICATA", async () => {
    const reprinted: unknown[] = [];
    const calls = mockApp({
      auth_session: () => cashierSession,
      hardware_retry_print: () => Promise.reject({ code: "print_not_pending" }),
      credit_reprint_receipt: (args) => {
        reprinted.push(args);
        return { printer: "Caisse 1", bytes: 900, raster: false };
      },
    });
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("La caisse est fermée");
    await emit("print-failed", {
      documentId: "e-versement",
      number: "RV2026-000001",
      code: "printer_timeout",
      versement: true,
    });
    expect(
      await screen.findByText("Le reçu de versement RV2026-000001 n'a pas été imprimé"),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Réimprimer" }));
    await waitFor(() => expect(reprinted).toEqual([{ entryId: "e-versement", lang: null }]));
    expect(calls).not.toContain("documents_reprint");
  });

  it("sends a kitchen ticket that did not print to the kitchen again, never as a receipt", async () => {
    const reprinted: unknown[] = [];
    const calls = mockApp({
      auth_session: () => cashierSession,
      kitchen_reprint: (args) => {
        reprinted.push(args);
        return null;
      },
    });
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("La caisse est fermée");
    await emit("print-failed", {
      documentId: "k-1",
      number: null,
      code: "printer_offline",
      versement: false,
      slip: "kitchen",
    });
    expect(await screen.findByText("Le bon de cuisine n'a pas été imprimé")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Réimprimer" }));
    await waitFor(() => expect(reprinted).toEqual([{ id: "k-1" }]));
    expect(calls).not.toContain("hardware_retry_print");
    expect(calls).not.toContain("documents_reprint");
  });

  it("prints the addition again when it did not print", async () => {
    const asked: unknown[] = [];
    mockApp({
      auth_session: () => cashierSession,
      dining_ticket_prebill: (args) => {
        asked.push(args);
        return null;
      },
    });
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("La caisse est fermée");
    await emit("print-failed", {
      documentId: "t-1",
      number: null,
      code: "printer_offline",
      versement: false,
      slip: "pre_bill",
    });
    expect(await screen.findByText("L'addition n'a pas été imprimée")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Réimprimer" }));
    await waitFor(() => expect(asked).toEqual([{ ticketId: "t-1", parts: null }]));
  });
});

describe("Backup alerts", () => {
  const healthy: BackupStatus = {
    lastAuto: null,
    newestAt: null,
    stale: false,
    databaseDamaged: false,
    mirrorError: null,
  };

  it("warns a cashier that the automatic backup failed, with nothing to open", async () => {
    mockApp({
      auth_session: () => cashierSession,
      backup_status: () => ({ ...healthy, lastAuto: { at: 1, ok: false, error: "io_error" } }),
    });
    render(<App />);
    const banner = await screen.findByRole("alert");
    expect(banner).toHaveTextContent("La sauvegarde automatique a échoué");
    expect(banner).toHaveTextContent("Vérifiez l'espace libre sur le disque");
    expect(within(banner).queryByRole("button")).toBeNull();
  });

  it("tells the owner the newest backup is too old and opens the backups", async () => {
    mockApp({
      auth_session: () => ownerSession,
      backup_status: () => ({ ...healthy, newestAt: backupFixture.at, stale: true }),
      backup_list: () => [backupFixture],
    });
    const user = userEvent.setup();
    render(<App />);
    const banner = await screen.findByRole("alert");
    expect(banner).toHaveTextContent("La sauvegarde la plus récente date du 28/09/2026.");
    await user.click(within(banner).getByRole("button", { name: "Ouvrir les sauvegardes" }));
    expect(await screen.findByRole("button", { name: "Sauvegarder maintenant" })).toBeInTheDocument();
  });

  it("stops the sale with a blocking alert when the database is damaged", async () => {
    mockApp({
      auth_session: () => cashierSession,
      backup_status: () => ({
        ...healthy,
        lastAuto: { at: 1, ok: false, error: "backup_invalid" },
        databaseDamaged: true,
      }),
    });
    const user = userEvent.setup();
    render(<App />);
    const dialog = await screen.findByRole("dialog", {
      name: "Les données de la boutique sont endommagées : arrêtez les ventes",
    });
    await user.keyboard("{Escape}");
    expect(dialog).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "J'ai compris" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getAllByRole("alert")[0]).toHaveTextContent("N'enregistrez plus de ventes.");
  });
});
