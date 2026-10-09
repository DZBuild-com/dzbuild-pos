import { clearMocks } from "@tauri-apps/api/mocks";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { appointments, cartWith, customerWithLang, order, packState, sessionWith } from "@/test/fixtures-m8";
import { AppointmentBook } from "./AppointmentBook";
import { OrderDialog } from "./OrderDialog";
import { OrderPage } from "./OrderPage";

afterEach(() => clearMocks());
const seller = sessionWith(["sales.create", "documents.quote", "documents.delivery"]);

describe("orders and appointments", () => {
  it("refuses a layaway deposit below the minimum before sending it", async () => {
    mockCommands({
      auth_session: () => seller,
      packs_state: () => packState({ modules: ["orders"] }),
      settings_get: () => 2000,
      deposits_quote: () => ({ amount: 500_000, stampDuty: 5_000, stampCharged: 5_000, toReceive: 505_000 }),
    });
    // the till hands the dialog its cart and the total its preview priced (the dialog never adds
    // lines up, nor imports the till's store from its lazy chunk)
    const till = cartWith(6_000_000, customerWithLang("fr"));
    const user = userEvent.setup();
    await renderM8(<OrderDialog {...till} onClose={() => {}} />, { session: seller });
    await user.click(await screen.findByRole("radio", { name: "Vente avec réservation" }));
    await user.type(screen.getByLabelText("Acompte"), "5000");
    expect(screen.getByText("Minimum 12 000,00 DA pour une vente avec réservation.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Enregistrer la commande" })).toBeDisabled();
  });

  it("offers the ways out of an order that holds a deposit", async () => {
    mockCommands({
      auth_session: () => seller,
      packs_state: () => packState({ modules: ["orders"] }),
      orders_get: () => order({ held: 1_000_000 }),
    });
    await renderM8(<OrderPage id="o1" />, { session: seller });
    expect(await screen.findByRole("button", { name: "Servir en caisse" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Clôturer" })).toBeDisabled();
    expect(screen.getByText("Remboursez les arrhes ou conservez-les avant de clôturer.")).toBeInTheDocument();
  });

  it("books a free slot and shows an overlap under the start time", async () => {
    let first = true;
    mockCommands({
      auth_session: () => seller,
      packs_state: () => packState({ modules: ["appointments"] }),
      appointments_day: () => appointments(),
      auth_list_users: () => [{ id: "u1", displayName: "Nadia" }],
      appointments_book: () => {
        if (first) {
          first = false;
          return Promise.reject({ code: "appointment_overlap", field: "startsAt" });
        }
        return { id: "a9" };
      },
    });
    const user = userEvent.setup();
    await renderM8(<AppointmentBook day="2026-09-28" />, { session: seller });
    await user.click(await screen.findByRole("button", { name: "Nadia 10:00 libre" }));
    await user.type(screen.getByLabelText("Nom du client"), "Sara");
    await user.click(screen.getByRole("button", { name: "Réserver" }));
    expect(
      await screen.findByText("Cette personne a déjà un rendez-vous à cette heure-là."),
    ).toBeInTheDocument();
  });
});
