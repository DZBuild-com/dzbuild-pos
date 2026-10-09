import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { sessionWith } from "@/test/fixtures-m8";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { RestaurantSettings } from "./RestaurantSettings";

const owner = sessionWith(["settings.company", "products.edit"]);

describe("the restaurant's own settings (final review)", () => {
  it("sets the kitchen tickets' language, the counter's tickets, the send on payment and the cover", async () => {
    const saved: Array<[unknown, unknown]> = [];
    const values: Record<string, unknown> = {
      "restaurant.kitchen_lang": "fr",
      "restaurant.predefined_tickets": ["Comptoir", "Terrasse"],
      "restaurant.send_on_pay": true,
      "restaurant.cover_product_id": null,
    };
    mockCommands({
      auth_session: () => owner,
      settings_get: ({ key }) => values[String(key)] ?? null,
      settings_set: ({ key, value }) => {
        saved.push([key, value]);
        values[String(key)] = value;
        return value;
      },
      products_search: () => [
        {
          id: "p-cover",
          reference: "COUV",
          nameAr: "غطاء",
          nameFr: "Couvert",
          nameEn: "Cover",
          sellPrice: 10_000,
        },
      ],
    });
    const user = userEvent.setup();
    await renderM8(<RestaurantSettings />, { session: owner });
    await screen.findByText("Comptoir");
    await user.click(screen.getByRole("radio", { name: "العربية" }));
    await user.type(screen.getByLabelText("Nouveau ticket du comptoir"), "Bar");
    await user.click(screen.getByRole("button", { name: "Ajouter" }));
    await user.click(screen.getByRole("button", { name: "Retirer Terrasse" }));
    await user.click(
      screen.getByRole("checkbox", {
        name: "Envoyer en cuisine les lignes non envoyées quand le ticket est encaissé",
      }),
    );
    await user.type(screen.getByLabelText("Produit vendu par couvert"), "couv");
    await user.click(await screen.findByRole("button", { name: /Couvert/ }));
    await waitFor(() =>
      expect(saved).toEqual([
        ["restaurant.kitchen_lang", "ar"],
        ["restaurant.predefined_tickets", ["Comptoir", "Terrasse", "Bar"]],
        ["restaurant.predefined_tickets", ["Comptoir", "Bar"]],
        ["restaurant.send_on_pay", false],
        ["restaurant.cover_product_id", "p-cover"],
      ]),
    );
  });
});
