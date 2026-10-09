import { configure, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "@/app/App";
import { useCart } from "@/features/pos/cart-store";
import type { LanStatus, Session } from "@/lib/ipc/types";
import { cashierSession, companyFixture, ownerTile, packFixture, settingsValues } from "@/test/fixtures";
import { activeStatus } from "@/test/fixtures-m9";
import {
  cashSessionFixture,
  categoriesFixture,
  previewFixture,
  priceLevelsFixture,
  productHitsFixture,
  saleDoneFixture,
  scanHitFixture,
  summaryFixture,
  taxRatesFixture,
  zReportFixture,
} from "@/test/fixtures-m1";
import {
  clientStatusFixture,
  lanHostStatusFixture,
  lanStandaloneFixture,
  lanTillStatusFixture,
} from "@/test/fixtures-m5";
import { mockCommands } from "@/test/render";
import { scanUntilLooked } from "@/test/scan";

// The whole app renders here, and a parallel Rust build on the same PC can slow it down past the
// 3 s of the other suites: these waits get 10 s, and each test 30 s.
configure({ asyncUtilTimeout: 10_000 });
vi.setConfig({ testTimeout: 30_000 });

/** The codes the screen asked the backend for (see `scanUntilLooked`). */
let lookups = 0;

type Tender = { method: string; amount: number };

/** The fake backend of one shift: who is signed in, whether the till is open and where this PC
 * stands on the shop's network (`lan`). A till whose main PC is down cannot close while operations
 * wait to be sent: its Z is an offline Z, approved by a manager. */
function backend(lan: LanStatus) {
  const state: { session: Session | null; open: boolean } = { session: null, open: false };
  const offline = lan.role === "client" && lan.client?.link === "offline";
  // the backend refuses the offline Z until a manager approved it
  let approved = false;
  const calls = mockCommands({
    app_startup_error: () => null,
    app_startup_notices: () => [],
    // M9: the licence (a lifetime licence: no banner, the till sells)
    license_status: () => activeStatus,
    setup_status: () => ({ completed: true, hasOwner: true, terminalCode: "C1", loginLang: "fr" }),
    auth_session: () => state.session,
    auth_list_users: () => [ownerTile, { ...ownerTile, id: "u-cashier", displayName: "Yacine" }],
    auth_login: () => {
      state.session = cashierSession;
      return cashierSession;
    },
    company_get: () => ({ ...companyFixture, defaultDocLang: "fr" }),
    settings_get: ({ key }) => settingsValues[String(key)] ?? (key === "cash.blind_close" ? false : null),
    settings_set: ({ value }) => value,
    datapack_get: () => packFixture,
    hardware_get: () => ({
      version: 1,
      printers: [
        { id: "p1", name: "Caisse 1", transport: { kind: "windows", printerName: "XP-80C" }, profile: {} },
      ],
      receiptPrinterId: "p1",
      receipt: {
        printByDefault: true,
        openDrawerOnCash: true,
        drawerBeforePrint: false,
        logo: true,
        secondLang: null,
      },
      scanner: { layout: "us" },
    }),
    cash_current: () => (state.open ? cashSessionFixture : null),
    cash_open_session: ({ openingFloat }) => {
      state.open = true;
      return { ...cashSessionFixture, openingFloat };
    },
    cash_movements: () => [],
    cash_sessions: () => [],
    cash_x_report: () => ({ ...summaryFixture, expectedCash: null }),
    cash_close_session: () => {
      if (offline) return Promise.reject({ code: "outbox_pending_at_close", details: { pending: 2 } });
      state.open = false;
      return { z: zReportFixture, backup: null, backupError: null };
    },
    cash_close_offline: () => {
      if (!approved) return Promise.reject({ code: "offline_z_needs_approval" });
      state.open = false;
      return { z: { ...zReportFixture, offlinePending: 2 }, backup: null, backupError: null };
    },
    auth_manager_override: () => {
      approved = true;
      return {
        approverId: "u-owner",
        approverName: "Amine Benali",
        permission: "cash.view_expected",
        grantedAt: 1,
      };
    },
    lan_status: () => lan,
    lan_sync_now: () => null,
    cash_print_z: () => ({ printer: "Caisse 1", bytes: 900, raster: false }),
    catalog_categories_list: () => categoriesFixture,
    catalog_price_levels_list: () => priceLevelsFixture,
    catalog_tax_rates_list: () => taxRatesFixture,
    products_grid: () => productHitsFixture,
    products_find_by_code: () => {
      lookups += 1;
      return { ...scanHitFixture, packQty: 2_000 };
    },
    sales_held: () => [],
    sales_preview: ({ input }) => {
      const tenders = (input as { tenders: Tender[] }).tenders;
      const cash = tenders.filter((p) => p.method === "cash").reduce((s, p) => s + p.amount, 0);
      const base = previewFixture(30_000, cash);
      const due = 30_500;
      return {
        ...base,
        settlement: {
          ...base.settlement,
          cashTendered: cash,
          paidTotal: cash,
          missing: Math.max(0, due - cash),
          change: Math.max(0, cash - due),
          totalDue: due,
          tenders,
        },
      };
    },
    sales_create: () => saleDoneFixture,
  });
  return calls;
}

/** `expected` appear in `calls` in this order (other calls may come between). */
function expectInOrder(calls: string[], expected: string[]) {
  let from = 0;
  for (const name of expected) {
    const at = calls.indexOf(name, from);
    expect(
      at,
      `${name} after ${expected.slice(0, expected.indexOf(name)).join(", ")}`,
    ).toBeGreaterThanOrEqual(0);
    from = at + 1;
  }
}

/** The shift on every LAN state the shell can see, with the pill the sale screen shows (none on a
 * single PC). */
const LAN_CASES: [string, LanStatus, string | null][] = [
  ["a single PC", lanStandaloneFixture, null],
  ["a main PC", lanHostStatusFixture, "Postes de caisse connectés : 1 / 2"],
  ["a till online", lanTillStatusFixture, "Connecté"],
  [
    "a till offline",
    { ...lanTillStatusFixture, client: { ...clientStatusFixture, link: "offline", pending: 2 } },
    "Hors ligne · 2 opérations à envoyer",
  ],
];

describe("a shift", () => {
  beforeEach(() => useCart.getState().clear("fr"));

  // A whole shift crosses five screens: under a full parallel run it outlives the 5 s default.
  it.each(LAN_CASES)(
    "a cashier on %s signs in, opens the till, sells, closes and prints the Z",
    { timeout: 20_000 },
    async (_, lan, pill) => {
      const offline = lan.role === "client" && lan.client?.link === "offline";
      const calls = backend(lan);
      const user = userEvent.setup();
      render(<App />);

      // sign in with the PIN
      await user.click(await screen.findByRole("button", { name: /Yacine/ }));
      await waitFor(() => expect(screen.getByRole("group", { name: "Code PIN" })).toHaveFocus());
      await user.keyboard("[Digit4][Digit8][Digit2][Digit6]{Enter}");

      // the cashier lands on the sale screen, which asks to open the till first
      expect(await screen.findByText("La caisse est fermée")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Ouvrir la caisse" }));
      await user.click(await screen.findByRole("button", { name: "Compter les billets et les pièces" }));
      await user.type(screen.getByLabelText("Billets de 2 000 DA"), "5");
      await user.click(screen.getByRole("button", { name: "Ouvrir la caisse avec ce fond" }));

      // the sale screen says where this PC stands on the network (the cashier has no
      // `settings.data`: the pill is a status, not a link to the network settings)
      const field = await screen.findByLabelText("Scanner ou rechercher un produit");
      if (pill) expect(await screen.findByRole("status", { name: pill })).toBeInTheDocument();

      // scan, pay 500 DA, hand back 195 DA
      await scanUntilLooked(field, "6130000000017", () => lookups);
      await screen.findByText("2 × Lait Candia 1 L");
      await user.keyboard("{F12}");
      const pay = await screen.findByRole("dialog", { name: "Encaissement" });
      for (const key of ["5", "0", "0"]) await user.click(within(pay).getByRole("button", { name: key }));
      await waitFor(() => expect(within(pay).getByRole("button", { name: /Valider/ })).toBeEnabled());
      await user.keyboard("{Enter}");
      await screen.findByRole("dialog", { name: "Vente encaissée" });
      expect(screen.getByRole("status", { name: "Rendu" })).toHaveTextContent("195,00");
      await user.click(screen.getByRole("button", { name: /Nouvelle vente/ }));

      // Esc → Menu → Caisse, then the close with the count sheet and the Z
      await user.keyboard("{Escape}");
      await user.click(
        await within(await screen.findByRole("dialog", { name: "Menu" })).findByRole("button", {
          name: "Caisse",
        }),
      );
      await user.click(await screen.findByRole("button", { name: "Clôturer la caisse" }));
      const close = await screen.findByRole("dialog");
      await user.type(within(close).getByLabelText("Billets de 2 000 DA"), "5");
      await user.click(within(close).getByRole("button", { name: "Clôturer et imprimer le Z" }));
      if (offline) {
        // the main PC is down: the Z waits for the sending, or a manager approves an offline Z
        expect(await within(close).findByText("Z hors ligne")).toBeInTheDocument();
        await user.click(
          within(close).getByRole("button", { name: "Faire un Z hors ligne (accord du gérant)" }),
        );
        expect(await screen.findByText("Accord d'un responsable")).toBeInTheDocument();
        await user.click(screen.getByRole("button", { name: /Amine Benali/ }));
        await user.keyboard("2580");
        await user.click(screen.getByRole("button", { name: "Autoriser" }));
        expect(await screen.findByText("ZC1-000031")).toBeInTheDocument();
        expect(screen.getByText("Z hors ligne")).toBeInTheDocument();
      } else {
        expect(await screen.findByText("ZC1-000031")).toBeInTheDocument();
      }
      expect(await screen.findByText("Rapport Z imprimé")).toBeInTheDocument();

      expectInOrder(calls, [
        "auth_login",
        "cash_open_session",
        "products_find_by_code",
        "sales_create",
        offline ? "cash_close_offline" : "cash_close_session",
        "cash_print_z",
      ]);
      expect(calls.filter((c) => c === "sales_create")).toHaveLength(1);
    },
  );
});
