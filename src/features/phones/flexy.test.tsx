import { clearMocks } from "@tauri-apps/api/mocks";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { flexyOperators, packState, sessionWith } from "@/test/fixtures-m8";
import { FlexyScreen } from "./FlexyScreen";

afterEach(() => clearMocks());

it("buys float for an operator and shows the new balance", async () => {
  const sent: unknown[] = [];
  const manager = sessionWith(["cash.paid_in_out", "reports.cash", "sales.create"]);
  mockCommands({
    auth_session: () => manager,
    packs_state: () => packState({ modules: ["flexy"] }),
    flexy_operators: () => flexyOperators(),
    flexy_balances: () => [["mobilis", 1_000_000]],
    flexy_entries: () => [],
    flexy_purchase: (args) => {
      sent.push(args);
      return { id: "f1", operator: "mobilis", kind: "purchase", amount: 2_000_000, cost: 1_920_000 };
    },
  });
  const user = userEvent.setup();
  await renderM8(<FlexyScreen />, { session: manager });
  await user.click(await screen.findByRole("button", { name: "Acheter du stock" }));
  await user.click(screen.getByRole("radio", { name: /Mobilis/ }));
  await user.type(screen.getByLabelText("Montant crédité"), "20000");
  await user.type(screen.getByLabelText("Prix payé"), "19200");
  await user.click(screen.getByRole("button", { name: "Enregistrer l'achat" }));
  await waitFor(() =>
    expect(sent[0]).toMatchObject({ input: { operator: "mobilis", amount: 2_000_000, cost: 1_920_000 } }),
  );
});
