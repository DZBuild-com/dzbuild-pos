import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { CashScreen } from "@/features/cash/CashScreen";
import { qk } from "@/lib/ipc/keys";
import { ownerSession, ownerTile, settingsValues } from "@/test/fixtures";
import { cashSessionFixture, summaryFixture, zReportFixture } from "@/test/fixtures-m1";
import { mockCommands, renderWithProviders } from "@/test/render";

const ready = (blockers: unknown[]) => ({
  currentVersion: "0.6.0",
  channel: "stable",
  autoCheck: true,
  autoDownload: true,
  lastCheckAt: 1_790_593_200_000,
  lastCheckError: null,
  offer: { version: "0.6.1", notes: null, pubDate: null, decision: { kind: "offer" } },
  download: { kind: "ready" },
  blockers,
  installing: false,
});

const thisDrawer = {
  kind: "cashSessionOpen",
  terminalCode: "C1",
  userName: "Amine Benali",
  openedAt: 1_790_578_800_000,
};

describe("the offer after the Z", () => {
  it("reads the drawers again once this one is closed", async () => {
    let current = ready([thisDrawer]);
    mockCommands({
      auth_list_users: () => [ownerTile],
      cash_current: () => cashSessionFixture,
      cash_movements: () => [],
      cash_sessions: () => [],
      cash_x_report: () => summaryFixture,
      settings_get: ({ key }) => settingsValues[String(key)] ?? null,
      cash_close_session: () => {
        current = ready([]);
        return { z: zReportFixture, backup: null, backupError: null };
      },
      cash_print_z: () => ({ printer: "Caisse 1", bytes: 800, raster: false }),
      updates_status: () => current,
    });
    const user = userEvent.setup();
    const { client } = renderWithProviders(<CashScreen session={ownerSession} />);
    // The top bar's pill read the status while this drawer was still open.
    client.setQueryData(qk.updates, current);
    await user.click(await screen.findByRole("button", { name: "Clôturer la caisse" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Clôturer et imprimer le Z" }));
    expect(
      await screen.findByText(
        "La version 0.6.1 est prête. L'installer maintenant ? DZBuild POS redémarrera.",
      ),
    ).toBeInTheDocument();
  });

  it("reads them again after an offline Z on a till too (M6 merge with M5)", async () => {
    let current = ready([thisDrawer]);
    mockCommands({
      auth_list_users: () => [ownerTile],
      cash_current: () => cashSessionFixture,
      cash_movements: () => [],
      cash_sessions: () => [],
      cash_x_report: () => summaryFixture,
      settings_get: ({ key }) => settingsValues[String(key)] ?? null,
      // the main PC is down at closing time: the owner closes an offline Z
      cash_close_session: () => Promise.reject({ code: "outbox_pending_at_close", details: { pending: 2 } }),
      cash_close_offline: () => {
        current = ready([]);
        return { z: zReportFixture, backup: null, backupError: null };
      },
      cash_print_z: () => ({ printer: "Caisse 1", bytes: 800, raster: false }),
      lan_sync_now: () => null,
      updates_status: () => current,
    });
    const user = userEvent.setup();
    const { client } = renderWithProviders(<CashScreen session={ownerSession} />);
    client.setQueryData(qk.updates, current);
    await user.click(await screen.findByRole("button", { name: "Clôturer la caisse" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Clôturer et imprimer le Z" }));
    await user.click(
      await within(dialog).findByRole("button", { name: "Faire un Z hors ligne (accord du gérant)" }),
    );
    expect(await screen.findByRole("dialog", { name: "Caisse clôturée" })).toBeInTheDocument();
    expect(
      await screen.findByText(
        "La version 0.6.1 est prête. L'installer maintenant ? DZBuild POS redémarrera.",
      ),
    ).toBeInTheDocument();
  });
});
