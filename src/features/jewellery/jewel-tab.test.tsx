import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { renderM8 } from "@/test/render-m8";
import { sessionWith } from "@/test/fixtures-m8";
import { productFixture } from "@/test/fixtures-m1";
import { JewelTab } from "./JewelTab";

const editor = sessionWith(["products.view", "products.edit"]);

describe("the product's jewel tab", () => {
  it("sends the weight in milligrams and the making charge in centimes", async () => {
    const saved: unknown[] = [];
    await renderM8(<JewelTab product={{ ...productFixture, id: "p-ring" }} editable />, {
      session: editor,
      commands: {
        jewellery_piece_get: () => null,
        jewellery_piece_set: (args) => {
          saved.push(args);
          return null;
        },
      },
    });
    const user = userEvent.setup();
    expect(await screen.findByText(/pas encore un bijou/)).toBeInTheDocument();
    expect(screen.getByText("18 carats")).toBeInTheDocument();
    await user.type(screen.getByLabelText("Poids (g)"), "4,25");
    await user.type(screen.getByLabelText("Façon"), "1500");
    await user.type(screen.getByLabelText("Poinçon"), "AG 750");
    await user.click(screen.getByRole("button", { name: "Enregistrer le bijou" }));
    await waitFor(() =>
      expect(saved[0]).toEqual({
        productId: "p-ring",
        input: {
          metal: "gold",
          titrePermille: 750,
          weightMg: 4_250,
          facon: 150_000,
          stones: null,
          hallmark: "AG 750",
          pricedByWeight: true,
        },
      }),
    );
  });

  it("shows the live price of a piece priced by weight, from the backend", async () => {
    await renderM8(<JewelTab product={{ ...productFixture, id: "p-ring" }} editable={false} />, {
      session: sessionWith(["products.view"]),
      commands: {
        jewellery_piece_get: () => ({
          productId: "p-ring",
          metal: "gold",
          titrePermille: 750,
          weightMg: 4_250,
          facon: 500_000,
          stones: null,
          hallmark: null,
          pricedByWeight: true,
          livePrice: 6_662_500,
          updatedAt: 1_790_596_800_000,
        }),
      },
    });
    expect(await screen.findByText("Prix actuel : 66 625,00 DA")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Enregistrer le bijou" })).toBeNull();
  });
});
