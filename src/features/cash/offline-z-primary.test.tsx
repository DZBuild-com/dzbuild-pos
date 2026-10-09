import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { CashScreen } from "@/features/cash/CashScreen";
import { cashierSession, ownerTile, settingsValues } from "@/test/fixtures";
import { cashSessionFixture, summaryFixture } from "@/test/fixtures-m1";
import { mockCommands, renderWithProviders } from "@/test/render";

/** Spec §10.4: `primary` is "one per screen, the action that moves money or saves". */
function primaryButtons(container: HTMLElement) {
  return within(container)
    .getAllByRole("button")
    .filter((button) => button.classList.contains("bg-brand"));
}

describe("CloseSessionDialog: the offline Z panel", () => {
  it("leaves one primary button, the offline Z, and gives it back to the close once the cashier waits", async () => {
    mockCommands({
      auth_session: () => cashierSession,
      auth_list_users: () => [ownerTile],
      cash_current: () => cashSessionFixture,
      cash_movements: () => [],
      cash_sessions: () => [],
      settings_get: ({ key }) => settingsValues[String(key)] ?? (key === "cash.blind_close" ? false : null),
      cash_x_report: () => ({ ...summaryFixture, expectedCash: null }),
      cash_close_session: () => Promise.reject({ code: "outbox_pending_at_close", details: { pending: 3 } }),
      lan_sync_now: () => null,
    });
    const user = userEvent.setup();
    renderWithProviders(<CashScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: "Clôturer la caisse" }));
    const dialog = await screen.findByRole("dialog");
    const closeButton = within(dialog).getByRole("button", { name: "Clôturer et imprimer le Z" });
    expect(primaryButtons(dialog)).toEqual([closeButton]);

    await user.type(within(dialog).getByLabelText("Billets de 2 000 DA"), "39");
    await user.click(closeButton);
    const offlineZ = await within(dialog).findByRole("button", {
      name: "Faire un Z hors ligne (accord du gérant)",
    });
    expect(primaryButtons(dialog)).toEqual([offlineZ]);
    // the close is still offered (the operations may have arrived since), just not as the primary
    expect(within(dialog).getByRole("button", { name: "Clôturer et imprimer le Z" })).toBeEnabled();

    await user.click(within(dialog).getByRole("button", { name: "Attendre l'envoi" }));
    expect(primaryButtons(dialog)).toEqual([
      within(dialog).getByRole("button", { name: "Clôturer et imprimer le Z" }),
    ]);
  });
});
