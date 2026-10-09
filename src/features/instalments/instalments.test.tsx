import { clearMocks } from "@tauri-apps/api/mocks";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { customerHit, instalmentQuote, packState, plan, sessionWith } from "@/test/fixtures-m8";
import { InstalmentWizard } from "./InstalmentWizard";
import { PlanPage } from "./PlanPage";

afterEach(() => clearMocks());
const seller = sessionWith(["sales.create", "credit.view", "credit.collect", "customers.view"]);

describe("instalments", () => {
  it("previews the schedule, offers only neutral documents and signs", async () => {
    const signed: unknown[] = [];
    mockCommands({
      auth_session: () => seller,
      packs_state: () => packState({ modules: ["instalments"] }),
      customers_search: () => [customerHit("Farid Bouzid")],
      instalments_quote: () => instalmentQuote(),
      instalments_sign: (args) => {
        signed.push(args);
        return {
          signed: { plan: plan(), sale: { document: { id: "d1", number: "TK2026-000050" } } },
          printQueued: true,
        };
      },
    });
    const user = userEvent.setup();
    await renderM8(<InstalmentWizard initialCart={{ productId: "p-fridge", qty: 1_000 }} />, {
      session: seller,
    });
    await user.type(await screen.findByLabelText("Client"), "Farid");
    await user.click(await screen.findByRole("option", { name: /Farid Bouzid/ }));
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    await user.clear(screen.getByLabelText("Apport initial"));
    await user.type(screen.getByLabelText("Apport initial"), "30000");
    await user.selectOptions(screen.getByLabelText("Nombre d'échéances"), "6");
    const schedule = await screen.findByRole("table", { name: "Échéancier" });
    expect(within(schedule).getAllByRole("row")).toHaveLength(1 + 6);
    expect(screen.getByText("À encaisser maintenant : 30 300,00 DA")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    const kinds = screen.getByLabelText("Type de pièce");
    expect(within(kinds).queryByRole("option", { name: /chèque/i })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Signer et encaisser l'apport" }));
    await waitFor(() => expect(signed[0]).toMatchObject({ input: { quote: { count: 6, down: 3_000_000 } } }));
  });

  it("shows the dues' states and collects the oldest unpaid one", async () => {
    const paid: unknown[] = [];
    mockCommands({
      auth_session: () => seller,
      packs_state: () => packState({ modules: ["instalments"] }),
      instalments_get: () => plan({ paidDues: 1, partial: 500_000, overdueDays: 36 }),
      privacy_consents: () => [],
      instalments_collect: (args) => {
        paid.push(args);
        return { collected: { entry: { id: "e1", receiptNumber: "RV2026-000010" } }, printQueued: true };
      },
    });
    const user = userEvent.setup();
    await renderM8(<PlanPage id="pl1" />, { session: seller });
    expect(await screen.findByText("En retard de 36 jours")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Encaisser une échéance" }));
    // M0's amount field shows centimes without grouping (`formatAmountInput`), like every
    // pre-filled amount of the app
    expect(screen.getByLabelText("Montant")).toHaveValue("10000,00");
    await user.click(screen.getByRole("button", { name: "Encaisser" }));
    // the command's argument is `payment` (`instalments_collect(planId, payment, printReceipt)`)
    await waitFor(() => expect(paid[0]).toMatchObject({ planId: "pl1", payment: { amount: 1_000_000 } }));
  });

  it("goes back one step on Esc on its own page (no dialog around it)", async () => {
    mockCommands({
      auth_session: () => seller,
      packs_state: () => packState({ modules: ["instalments"] }),
      customers_search: () => [customerHit("Farid Bouzid")],
      instalments_quote: () => instalmentQuote(),
    });
    const user = userEvent.setup();
    await renderM8(<InstalmentWizard initialCart={{ productId: "p-fridge", qty: 1_000 }} />, {
      session: seller,
    });
    await user.type(await screen.findByLabelText("Client"), "Farid");
    await user.click(await screen.findByRole("option", { name: /Farid Bouzid/ }));
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    await screen.findByRole("table", { name: "Échéancier" });
    await user.click(screen.getByLabelText("Apport initial"));
    await user.keyboard("{Escape}");
    expect(screen.getByText("Étape 2 sur 5")).toBeInTheDocument();
  });
});
