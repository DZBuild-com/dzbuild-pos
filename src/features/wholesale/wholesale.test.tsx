import { clearMocks } from "@tauri-apps/api/mocks";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { customerPrices, packState, round, sessionWith } from "@/test/fixtures-m8";
import { CustomerPrices } from "./CustomerPrices";
import { RoundPage } from "./RoundPage";

afterEach(() => clearMocks());
const manager = sessionWith([
  "documents.delivery",
  "credit.collect",
  "customers.view",
  "products.price",
  "stock.transfer",
]);

describe("wholesale", () => {
  it("shows the agreed price against the list price", async () => {
    mockCommands({
      auth_session: () => manager,
      packs_state: () => packState({ modules: ["wholesale"] }),
      wholesale_prices: () => customerPrices(),
    });
    await renderM8(<CustomerPrices customerId="c1" />, { session: manager });
    const row = (await screen.findByText("Eau minérale 1,5 L")).closest("tr");
    expect(within(row as HTMLElement).getByText("35,00 DA")).toBeInTheDocument();
    expect(within(row as HTMLElement).getByText("−12,5 %")).toBeInTheDocument();
  });

  it("reports a stop and refuses more than the stop's due", async () => {
    const reported: unknown[] = [];
    mockCommands({
      auth_session: () => manager,
      packs_state: () => packState({ modules: ["wholesale", "delivery_rounds"] }),
      rounds_get: () => round({ status: "out" }),
      rounds_report_stop: (args) => {
        reported.push(args);
        return round({ status: "out", collectedFirst: 500_000 });
      },
    });
    const user = userEvent.setup();
    await renderM8(<RoundPage id="r1" />, { session: manager });
    await user.click(await screen.findByRole("button", { name: "Arrêt 1 — Épicerie Amine" }));
    await user.click(screen.getByRole("radio", { name: "Livré" }));
    await user.type(screen.getByLabelText("Montant encaissé"), "6000");
    expect(screen.getByText("Le client doit 5 000,00 DA sur ce document.")).toBeInTheDocument();
    await user.clear(screen.getByLabelText("Montant encaissé"));
    await user.type(screen.getByLabelText("Montant encaissé"), "5000");
    await user.click(screen.getByRole("button", { name: "Enregistrer l'arrêt" }));
    await waitFor(() =>
      expect(reported[0]).toMatchObject({
        id: "r1",
        report: { seq: 1, status: "delivered", collected: 500_000, method: "cash" },
      }),
    );
  });

  it("closes a round after showing the cash to hand in", async () => {
    const closed: string[] = [];
    mockCommands({
      auth_session: () => manager,
      packs_state: () => packState({ modules: ["wholesale", "delivery_rounds"] }),
      rounds_get: () => round({ status: "out", collectedFirst: 500_000 }),
      rounds_close_preview: () => ({ collected: [["cash", 500_000]], cashToHandIn: 505_000, failed: 0 }),
      rounds_close: (args) => {
        closed.push(String(args.id));
        return { round: { round: round({ status: "closed" }), versements: [] }, printQueued: true };
      },
    });
    const user = userEvent.setup();
    await renderM8(<RoundPage id="r1" />, { session: manager });
    await user.click(await screen.findByRole("button", { name: "Clôturer la tournée" }));
    expect(await screen.findByText("Espèces à remettre : 5 050,00 DA")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Confirmer la clôture" }));
    await waitFor(() => expect(closed).toEqual(["r1"]));
  });
});
