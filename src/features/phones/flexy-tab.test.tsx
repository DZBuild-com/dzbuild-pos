import type { Module } from "@/lib/ipc/types";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CashScreen } from "@/features/cash/CashScreen";
import { cashSessionFixture } from "@/test/fixtures-m1";
import { flexyOperators, packState, sessionWith } from "@/test/fixtures-m8";
import { mockCommands, renderWithProviders } from "@/test/render";

const manager = sessionWith(["cash.open", "cash.close", "cash.paid_in_out", "sales.create"]);

function mockCash(modules: Module[]) {
  return mockCommands({
    auth_session: () => manager,
    auth_list_users: () => [],
    cash_current: () => cashSessionFixture,
    packs_state: () => packState({ modules }),
    flexy_operators: () => flexyOperators(),
    flexy_balances: () => flexyOperators().map((o) => [o.code, o.balance]),
  });
}

describe("the cash page's flexy tab", () => {
  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
  });

  it("shows the float per operator while the flexy module is on, the low ones flagged", async () => {
    const calls = mockCash(["flexy"]);
    const user = userEvent.setup();
    renderWithProviders(<CashScreen session={manager} />, { route: "/cash" });
    await user.click(await screen.findByRole("tab", { name: "Flexy" }));
    expect(await screen.findByText("Stock Mobilis")).toBeInTheDocument();
    // Djezzy (2 500,00 DA) and Ooredoo (0) are under 5 000 DA
    expect(screen.getAllByText("Stock bas")).toHaveLength(2);
    // a manager without `reports.cash` buys float but does not read the ledger
    expect(screen.getByRole("button", { name: "Acheter du stock" })).toBeEnabled();
    expect(calls).not.toContain("flexy_entries");
  });

  it("has no flexy tab while the module is off", async () => {
    const calls = mockCash([]);
    renderWithProviders(<CashScreen session={manager} />, { route: "/cash?tab=flexy" });
    await waitFor(() => expect(calls).toContain("packs_state"));
    expect(await screen.findByRole("heading", { name: "Caisse" })).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Flexy" })).toBeNull();
    expect(screen.queryByText("Stock Mobilis")).toBeNull();
  });
});
