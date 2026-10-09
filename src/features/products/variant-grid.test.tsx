import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { ReceiveDialog } from "@/features/purchases/ReceiveDialog";
import { lotTrackedExtrasFixture, supplierFixture } from "@/test/fixtures-m2";
import { oxfordShirt, packState, sessionWith, variantGrid } from "@/test/fixtures-m8";
import { VariantGrid } from "./VariantGrid";

afterEach(() => {
  cleanup();
  clearMocks();
  // the axes chosen are remembered per model on this PC
  window.localStorage.removeItem("variantGrid.p-oxford");
});

it("shows stock per size and colour and turns the grid", async () => {
  const seen: Record<string, unknown>[] = [];
  mockCommands({
    auth_session: () => sessionWith(["products.view"]),
    packs_state: () => packState({ modules: ["variant_grid"] }),
    variant_grid: (args) => {
      seen.push(args);
      return variantGrid({ rowAxis: String(args.rowAxis ?? "Couleur") });
    },
  });
  const user = userEvent.setup();
  await renderM8(<VariantGrid parentId="p-oxford" />);
  const grid = await screen.findByRole("grid");
  expect(within(grid).getByRole("gridcell", { name: "Blanc S 4" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Inverser les axes" }));
  expect(seen.at(-1)).toMatchObject({ rowAxis: "Taille" });
  // the sizes in rows now, and the choice kept for this model on this PC
  expect(await within(grid).findByRole("gridcell", { name: "S Blanc 4" })).toBeInTheDocument();
  expect(window.localStorage.getItem("variantGrid.p-oxford")).toBe("Taille");
});

it("takes a quantity per cell and adds one line per filled cell", async () => {
  mockCommands({
    auth_session: () => sessionWith(["stock.count"]),
    packs_state: () => packState({ modules: ["variant_grid"] }),
    variant_grid: () => variantGrid(),
  });
  const added: unknown[] = [];
  const user = userEvent.setup();
  await renderM8(<VariantGrid parentId="p-oxford" onEnter={(lines) => added.push(lines)} />);
  const grid = await screen.findByRole("grid");
  await user.type(within(grid).getByRole("textbox", { name: "S Bleu" }), "3");
  await user.type(within(grid).getByRole("textbox", { name: "M Blanc" }), "1");
  await user.click(screen.getByRole("button", { name: "Ajouter les lignes" }));
  expect(added).toEqual([
    [
      { variantId: "v-s-bleu", qty: 3_000 },
      { variantId: "v-m-blanc", qty: 1_000 },
    ],
  ]);
});

it("forgets an axis kept on this PC that the model no longer has", async () => {
  window.localStorage.setItem("variantGrid.p-oxford", "Saison");
  const seen: unknown[] = [];
  mockCommands({
    auth_session: () => sessionWith(["products.view"]),
    variant_grid: (args) => {
      seen.push(args.rowAxis);
      if (args.rowAxis === "Saison") {
        return Promise.reject({ code: "variant_axes_invalid", field: "rowAxis" });
      }
      return variantGrid();
    },
  });
  await renderM8(<VariantGrid parentId="p-oxford" />);
  const grid = await screen.findByRole("grid");
  expect(within(grid).getByRole("gridcell", { name: "M Bleu 3" })).toBeInTheDocument();
  expect(seen).toEqual(["Saison", null]);
  expect(window.localStorage.getItem("variantGrid.p-oxford")).toBeNull();
});

it("receives a variant model by grid: one receipt line per filled cell", async () => {
  const variant = (id: string, nameFr: string) => ({ ...oxfordShirt, id, nameFr, kind: "variant" as const });
  mockCommands({
    auth_session: () => sessionWith(["purchases.manage"]),
    packs_state: () => packState({ modules: ["variant_grid"] }),
    products_search: () => [oxfordShirt],
    products_variants: () => [
      variant("v-s-bleu", "Chemise Oxford S Bleu"),
      variant("v-m-blanc", "Chemise Oxford M Blanc"),
    ],
    products_extras_get: ({ id }) => ({
      ...lotTrackedExtrasFixture,
      productId: String(id),
      trackLots: false,
    }),
    variant_grid: () => variantGrid(),
  });
  const user = userEvent.setup();
  await renderM8(<ReceiveDialog supplier={supplierFixture} open onClose={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "Saisir par grille" }));
  await user.type(await screen.findByRole("searchbox", { name: "Modèle à déclinaisons" }), "Chemise");
  await user.click(await screen.findByRole("button", { name: "Chemise Oxford" }));
  const grid = await screen.findByRole("grid");
  await user.type(within(grid).getByRole("textbox", { name: "S Bleu" }), "3");
  await user.type(within(grid).getByRole("textbox", { name: "M Blanc" }), "2");
  await user.click(screen.getByRole("button", { name: "Ajouter les lignes" }));
  expect(await screen.findByText("Chemise Oxford S Bleu")).toBeInTheDocument();
  expect(screen.getByText("Chemise Oxford M Blanc")).toBeInTheDocument();
  await waitFor(() => expect(screen.queryByRole("grid")).not.toBeInTheDocument());
});
