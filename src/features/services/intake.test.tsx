import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { customerFixture } from "@/test/fixtures-m2";
import { serviceBoard, serviceTicket, sessionWith } from "@/test/fixtures-m8";
import { renderM8 } from "@/test/render-m8";
import { ServicesScreen } from "./ServicesScreen";

const cashier = sessionWith(["sales.create", "customers.view"]);

describe("a new drop-off", () => {
  it("takes the item in for a customer, refuses a wrong IMEI, then offers the deposit with its duty", async () => {
    const intakes: unknown[] = [];
    const deposits: unknown[] = [];
    const user = userEvent.setup();
    await renderM8(<ServicesScreen />, {
      route: "/",
      session: cashier,
      commands: {
        service_board: () => serviceBoard(),
        auth_list_users: () => [],
        customers_search: () => [{ ...customerFixture, id: "c-nassim", name: "Nassim Ould Ali" }],
        customers_get: () => ({ ...customerFixture, id: "c-nassim", name: "Nassim Ould Ali", lang: "ar" }),
        service_intake: (args) => {
          intakes.push(args);
          return serviceTicket({ id: "st5", number: "RP2026-000005" });
        },
        deposits_quote: () => ({
          amount: 300_000,
          stampDuty: 3_000,
          stampCharged: 3_000,
          toReceive: 303_000,
        }),
        deposits_take: (args) => {
          deposits.push(args);
          return { id: "dp1" };
        },
      },
    });
    await screen.findByRole("region", { name: "Prêt" });
    await user.keyboard("{Control>}n{/Control}");
    const intake = await screen.findByRole("dialog", { name: "Nouveau dépôt" });
    await user.type(within(intake).getByRole("searchbox", { name: "Client" }), "nass");
    await user.click(await within(intake).findByRole("option", { name: /Nassim Ould Ali/ }));
    await user.type(within(intake).getByLabelText("Appareil ou article"), "Samsung A54");
    await user.type(within(intake).getByLabelText("Panne ou demande"), "Écran cassé");
    await user.type(within(intake).getByLabelText("N° de série / IMEI"), "356938035643808");
    await user.click(within(intake).getByRole("button", { name: "Enregistrer et imprimer le bon" }));
    expect(
      within(intake).getByText("Cet IMEI n'est pas valide : vérifiez les 15 chiffres."),
    ).toBeInTheDocument();
    expect(intakes).toEqual([]);

    await user.clear(within(intake).getByLabelText("N° de série / IMEI"));
    await user.type(within(intake).getByLabelText("N° de série / IMEI"), "356938035643809");
    await user.click(within(intake).getByRole("button", { name: "Enregistrer et imprimer le bon" }));
    await waitFor(() =>
      expect(intakes[0]).toMatchObject({
        input: {
          kind: "repair",
          customerId: "c-nassim",
          item: "Samsung A54",
          serialNo: "356938035643809",
          fault: "Écran cassé",
          // the customer's file says Arabic: his slip and his "ready" message
          lang: "ar",
        },
      }),
    );

    const deposit = await screen.findByRole("dialog", { name: "Arrhes" });
    expect(within(deposit).getByText("Bon RP2026-000005 — Nassim Ould Ali")).toBeInTheDocument();
    await user.type(within(deposit).getByLabelText("Montant"), "3000");
    expect(await within(deposit).findByText(/À encaisser : 3\s030,00/)).toBeInTheDocument();
    await user.click(within(deposit).getByRole("button", { name: "Encaisser les arrhes" }));
    await waitFor(() =>
      expect(deposits[0]).toMatchObject({
        input: { ownerType: "service_ticket", ownerId: "st5", amount: 300_000, method: "cash" },
      }),
    );
  });
});
