import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { CashScreen } from "@/features/cash/CashScreen";
import { setLanguage } from "@/lib/i18n";
import { cashierSession, ownerSession, ownerTile, settingsValues } from "@/test/fixtures";
import { cashSessionFixture, summaryFixture, zReportFixture } from "@/test/fixtures-m1";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";

function mockCash(extra: Record<string, Handler> = {}) {
  return mockCommands({
    auth_session: () => cashierSession,
    auth_list_users: () => [ownerTile, { ...ownerTile, id: "u-cashier", displayName: "Yacine" }],
    cash_current: () => cashSessionFixture,
    cash_movements: () => [],
    cash_sessions: () => [],
    settings_get: ({ key }) => settingsValues[String(key)] ?? (key === "cash.blind_close" ? false : null),
    ...extra,
  });
}

describe("CashScreen", () => {
  it("opens the till with a counted float", async () => {
    const received: unknown[] = [];
    mockCash({
      cash_current: () => null,
      cash_open_session: (args) => {
        received.push(args);
        return cashSessionFixture;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<CashScreen session={cashierSession} />);
    expect(await screen.findByText("La caisse est fermée")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Ouvrir la caisse" }));
    await user.click(screen.getByRole("button", { name: "Compter les billets et les pièces" }));
    await user.type(screen.getByLabelText("Billets de 2 000 DA"), "5");
    expect(screen.getByLabelText("Fond de caisse")).toHaveValue("10000,00");
    await user.click(screen.getByRole("button", { name: "Ouvrir la caisse avec ce fond" }));
    await waitFor(() => expect(received).toEqual([{ openingFloat: 1_000_000 }]));
  });

  it("records a paid-out with its reason", async () => {
    const received: unknown[] = [];
    mockCash({
      cash_move: (args) => {
        received.push(args);
        return { id: "m-1" };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<CashScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: "Sortie d'argent" }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText("Montant"), "500");
    await user.type(within(dialog).getByLabelText("Motif"), "Achat de sacs");
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer la sortie" }));
    await waitFor(() =>
      expect(received).toEqual([{ input: { kind: "paid_out", amount: 50_000, note: "Achat de sacs" } }]),
    );
  });

  it("a large variance forces a recount, then a manager", async () => {
    let attempts = 0;
    const overrides: unknown[] = [];
    const calls = mockCash({
      cash_x_report: () => ({ ...summaryFixture, expectedCash: null }),
      cash_close_session: () => {
        attempts += 1;
        return attempts <= 2
          ? Promise.reject({ code: "variance_needs_approval" })
          : { z: zReportFixture, backup: null, backupError: null };
      },
      auth_manager_override: (args) => {
        overrides.push(args);
        return {
          approverId: "u-owner",
          approverName: "Amine Benali",
          permission: "cash.view_expected",
          grantedAt: 1,
        };
      },
      cash_print_z: () => ({ printer: "Caisse 1", bytes: 800, raster: false }),
    });
    const user = userEvent.setup();
    renderWithProviders(<CashScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: "Clôturer la caisse" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).queryByText("Espèces attendues")).toBeNull(); // no cash.view_expected
    await user.type(within(dialog).getByLabelText("Billets de 2 000 DA"), "39");
    await user.click(within(dialog).getByRole("button", { name: "Clôturer et imprimer le Z" }));
    expect(
      await within(dialog).findByText(
        "L'écart dépasse le seuil autorisé. Recomptez la caisse, billet par billet.",
      ),
    ).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Billets de 2 000 DA")).toHaveValue("");
    await user.type(within(dialog).getByLabelText("Billets de 2 000 DA"), "39");
    await user.click(within(dialog).getByRole("button", { name: "Clôturer et imprimer le Z" }));
    expect(await screen.findByText("Accord d'un responsable")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Amine Benali/ }));
    await user.keyboard("2580");
    await user.click(screen.getByRole("button", { name: "Autoriser" }));
    expect(await screen.findByText("ZC1-000031")).toBeInTheDocument();
    expect(overrides).toEqual([
      { input: { approverId: "u-owner", pin: "2580", permission: "cash.view_expected" } },
    ]);
    await waitFor(() => expect(calls).toContain("cash_print_z"));
    expect(await screen.findByText("Rapport Z imprimé")).toBeInTheDocument();
  });

  it("shows the expected cash to a manager when blind close is off", async () => {
    mockCash({ auth_session: () => ownerSession, cash_x_report: () => summaryFixture });
    const user = userEvent.setup();
    renderWithProviders(<CashScreen session={ownerSession} />);
    await user.click(await screen.findByRole("button", { name: "Clôturer la caisse" }));
    expect(await screen.findByText("Espèces attendues")).toBeInTheDocument();
  });

  it("lists the kridi versements taken in cash on the X report (CRD-004)", async () => {
    mockCash({ cash_x_report: () => ({ ...summaryFixture, creditPayments: 101_000 }) });
    const user = userEvent.setup();
    renderWithProviders(<CashScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: "Rapport X" }));
    const x = await screen.findByRole("dialog", { name: "Rapport X" });
    const row = (await within(x).findByText("Versements kridi en espèces")).closest("div");
    expect(row).toHaveTextContent(/1\s010,00/);
  });

  it("lists the session's invoices and credit notes, whose payments the X counts (M3)", async () => {
    mockCash({
      cash_x_report: () => ({
        ...summaryFixture,
        invoices: 2,
        invoicesTtc: 1_190_000,
        creditNotes: 1,
        creditNotesTtc: -119_000,
      }),
    });
    const user = userEvent.setup();
    renderWithProviders(<CashScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: "Rapport X" }));
    const x = await screen.findByRole("dialog", { name: "Rapport X" });
    expect((await within(x).findByText("Factures (2)")).closest("div")).toHaveTextContent(/11\s900,00/);
    expect(within(x).getByText("Avoirs (1)").closest("div")).toHaveTextContent(/1\s190,00/);
  });

  it("a blind X shows the counts and the card total, and no drawer figure (CSH-004)", async () => {
    const calls = mockCash({
      cash_x_report: () => ({
        ...summaryFixture,
        session: { ...summaryFixture.session, openingFloat: 0 },
        salesTtc: 0,
        returnsTtc: 0,
        netTtc: 0,
        stampCharged: 0,
        byMethod: summaryFixture.byMethod.filter((m) => m.method !== "cash"),
        openingFloat: 0,
        cashSales: 0,
        cashRefunds: 0,
        expectedCash: null,
        blind: true,
      }),
    });
    const user = userEvent.setup();
    renderWithProviders(<CashScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: "Rapport X" }));
    const x = await screen.findByRole("dialog", { name: "Rapport X" });
    expect(await within(x).findByText(/Clôture à l'aveugle/)).toBeInTheDocument();
    expect(within(x).getByText("42")).toBeInTheDocument();
    expect(within(x).getByText("Carte CIB")).toBeInTheDocument();
    for (const hidden of [
      "Fond de caisse",
      "Ventes",
      "Net",
      "Entrées d'argent",
      "Sorties d'argent",
      "Versements kridi en espèces",
      "Dépenses",
      "Espèces attendues",
    ]) {
      expect(within(x).queryByText(hidden)).toBeNull();
    }
    // every drawer movement would add up to the expected cash: not even asked for, and the
    // float is not on the cash screen either
    expect(calls).not.toContain("cash_movements");
    expect(screen.queryByText("Fond de caisse")).toBeNull();
  });

  it("a till with operations to send closes an offline Z with a manager's approval", async () => {
    let offlineAttempts = 0;
    let approved = false;
    const calls = mockCash({
      cash_x_report: () => ({ ...summaryFixture, expectedCash: null }),
      cash_close_session: () => Promise.reject({ code: "outbox_pending_at_close", details: { pending: 3 } }),
      cash_close_offline: () => {
        offlineAttempts += 1;
        return approved
          ? { z: { ...zReportFixture, offlinePending: 3 }, backup: null, backupError: null }
          : Promise.reject({ code: "offline_z_needs_approval" });
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
      lan_sync_now: () => null,
      cash_print_z: () => ({ printer: "Caisse 1", bytes: 800, raster: false }),
    });
    const user = userEvent.setup();
    renderWithProviders(<CashScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: "Clôturer la caisse" }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText("Billets de 2 000 DA"), "39");
    await user.click(within(dialog).getByRole("button", { name: "Clôturer et imprimer le Z" }));
    expect(
      await within(dialog).findByText(
        "Opérations de ce poste pas encore arrivées au poste principal : 3. Vous pouvez attendre l'envoi, ou faire un Z hors ligne : le Z le dira et le poste principal le signalera au propriétaire.",
      ),
    ).toBeInTheDocument();
    await user.click(
      within(dialog).getByRole("button", { name: "Faire un Z hors ligne (accord du gérant)" }),
    );
    expect(await screen.findByText("Accord d'un responsable")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Amine Benali/ }));
    await user.keyboard("2580");
    await user.click(screen.getByRole("button", { name: "Autoriser" }));
    expect(await screen.findByText("ZC1-000031")).toBeInTheDocument();
    expect(screen.getByText("Z hors ligne")).toBeInTheDocument();
    // asked once, refused for the approval (twice: once to learn it, once under the dialog), then
    // closed under it
    expect(offlineAttempts).toBe(3);
    await waitFor(() => expect(calls).toContain("cash_print_z"));
  });

  it("an offline Z over the variance threshold is counted again before any manager is asked", async () => {
    const attempts: string[] = [];
    let approved = false;
    mockCash({
      cash_x_report: () => ({ ...summaryFixture, expectedCash: null }),
      cash_close_session: () => Promise.reject({ code: "outbox_pending_at_close", details: { pending: 3 } }),
      cash_close_offline: () => {
        attempts.push(approved ? "approved" : "bare");
        return approved
          ? { z: { ...zReportFixture, offlinePending: 3 }, backup: null, backupError: null }
          : Promise.reject({ code: "variance_needs_approval" });
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
      lan_sync_now: () => null,
      cash_print_z: () => ({ printer: "Caisse 1", bytes: 800, raster: false }),
    });
    const user = userEvent.setup();
    renderWithProviders(<CashScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: "Clôturer la caisse" }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText("Billets de 2 000 DA"), "39");
    await user.click(within(dialog).getByRole("button", { name: "Clôturer et imprimer le Z" }));
    const offline = await within(dialog).findByRole("button", {
      name: "Faire un Z hors ligne (accord du gérant)",
    });
    await user.click(offline);
    // the first count is refused (and journaled by the backend): no manager yet, a recount
    expect(
      await within(dialog).findByText(
        "L'écart dépasse le seuil autorisé. Recomptez la caisse, billet par billet.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("Accord d'un responsable")).toBeNull();
    expect(within(dialog).getByLabelText("Billets de 2 000 DA")).toHaveValue("");
    await user.type(within(dialog).getByLabelText("Billets de 2 000 DA"), "39");
    await user.click(
      within(dialog).getByRole("button", { name: "Faire un Z hors ligne (accord du gérant)" }),
    );
    expect(await screen.findByText("Accord d'un responsable")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Amine Benali/ }));
    await user.keyboard("2580");
    await user.click(screen.getByRole("button", { name: "Autoriser" }));
    expect(await screen.findByText("ZC1-000031")).toBeInTheDocument();
    expect(attempts).toEqual(["bare", "bare", "approved"]);
  });

  it("waiting for the operations to be sent keeps the count and asks to send now", async () => {
    const calls = mockCash({
      cash_x_report: () => ({ ...summaryFixture, expectedCash: null }),
      cash_close_session: () => Promise.reject({ code: "outbox_pending_at_close", details: { pending: 2 } }),
      lan_sync_now: () => null,
    });
    const user = userEvent.setup();
    renderWithProviders(<CashScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: "Clôturer la caisse" }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText("Billets de 2 000 DA"), "39");
    await user.click(within(dialog).getByRole("button", { name: "Clôturer et imprimer le Z" }));
    await user.click(await within(dialog).findByRole("button", { name: "Attendre l'envoi" }));
    await waitFor(() => expect(calls).toContain("lan_sync_now"));
    expect(within(dialog).getByLabelText("Billets de 2 000 DA")).toHaveValue("39");
  });

  it("reads right to left in Arabic", async () => {
    await setLanguage("ar");
    mockCash({ cash_current: () => null });
    renderWithProviders(<CashScreen session={cashierSession} />);
    expect(await screen.findByText("الصندوق مغلق")).toBeInTheDocument();
    expect(document.documentElement).toHaveAttribute("dir", "rtl");
    expect(screen.getByRole("button", { name: "فتح الصندوق" })).toBeInTheDocument();
  });
});
