import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockConvertFileSrc } from "@tauri-apps/api/mocks";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BulkEditDialog } from "@/features/products/BulkEditDialog";
import { BulkPriceDialog } from "@/features/products/BulkPriceDialog";
import { PriceBreaksEditor } from "@/features/products/PriceBreaksEditor";
import { PriceHelper } from "@/features/products/PriceHelper";
import { ProductExtrasPanel } from "@/features/products/ProductExtrasPanel";
import { ProductPhoto } from "@/features/products/ProductPhoto";
import { VariantsDialog } from "@/features/products/VariantsDialog";
import type { PriceChangeRow, PriceHelpInput, Product, SupplierHit } from "@/lib/ipc/types";
import { qk } from "@/lib/ipc/keys";
import { mediaUrl } from "@/lib/media";
import { categoriesFixture, priceLevelsFixture, productFixture, taxRatesFixture } from "@/test/fixtures-m1";
import { lotTrackedExtrasFixture } from "@/test/fixtures-m2";
import { managerSession, storekeeperSession } from "@/test/fixtures";
import { mockCommands, renderWithProviders } from "@/test/render";

const RETAIL = "00000000-0000-7000-8000-000000000201";
const WHOLESALE = "00000000-0000-7000-8000-000000000203";

beforeEach(() => {
  // jsdom has no layout: TanStack Virtual shows no row in a 0×0 scroller.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

/** The milk and the oil, 10 % dearer rounded to 5 DA (the backend's figures). */
const previewRows: PriceChangeRow[] = [
  {
    productId: "p-milk",
    reference: "LAIT-1L",
    nameAr: "حليب كانديا 1 لتر",
    nameFr: "Lait Candia 1 L",
    nameEn: "Candia milk 1 L",
    before: 15_000,
    after: 16_500,
  },
  {
    productId: "p-oil",
    reference: "HUILE-5L",
    nameAr: "زيت إليو 5 لتر",
    nameFr: "Huile Elio 5 L",
    nameEn: "Elio oil 5 L",
    before: 65_000,
    after: 71_500,
  },
];

const cevital: SupplierHit = {
  id: "s-cevital",
  code: "F0001",
  name: "Cevital",
  phone: null,
  wilaya: "16",
  balance: 0,
  archived: false,
};

describe("bulk prices", () => {
  it("shows the backend's new prices and applies nothing before Appliquer", async () => {
    const previews: unknown[] = [];
    const applied: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      catalog_categories_list: () => categoriesFixture,
      catalog_price_levels_list: () => priceLevelsFixture,
      suppliers_search: () => [cevital],
      products_bulk_price_preview: (args) => {
        previews.push(args.input);
        return previewRows;
      },
      products_bulk_price_apply: (args) => {
        applied.push(args.input);
        return 2;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<BulkPriceDialog productIds={[]} onClose={() => {}} />);
    await screen.findByRole("option", { name: "Crèmerie" });
    await user.selectOptions(screen.getByLabelText("Catégorie"), "c-dairy");
    await user.selectOptions(screen.getByLabelText("Changement"), "percent_up");
    await user.type(screen.getByLabelText("Pourcentage (%)"), "10");
    await user.selectOptions(screen.getByLabelText("Arrondi"), "500");
    expect(screen.getByRole("button", { name: "Appliquer" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Voir les nouveaux prix" }));
    const milk = await screen.findByRole("row", { name: /Lait Candia 1 L/ });
    expect(milk).toHaveTextContent("150,00 DA");
    expect(milk).toHaveTextContent("165,00 DA");
    expect(screen.getByRole("row", { name: /Huile Elio 5 L/ })).toHaveTextContent("715,00 DA");
    expect(applied).toEqual([]);
    await user.click(screen.getByRole("button", { name: "Appliquer" }));
    await waitFor(() => expect(applied).toHaveLength(1));
    expect(applied[0]).toEqual({
      productIds: [],
      categoryId: "c-dairy",
      brand: null,
      supplierId: null,
      priceLevelId: null,
      change: { kind: "percent", value: 1_000 },
      roundTo: 500,
    });
    expect(previews[0]).toEqual(applied[0]);
  });

  it("changes the selection again only after a new preview", async () => {
    const applied: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      catalog_categories_list: () => categoriesFixture,
      catalog_price_levels_list: () => priceLevelsFixture,
      suppliers_search: () => [cevital],
      products_bulk_price_preview: () => previewRows,
      products_bulk_price_apply: (args) => {
        applied.push(args.input);
        return 2;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<BulkPriceDialog productIds={["p-milk", "p-oil"]} onClose={() => {}} />);
    expect(screen.getByText("Produits choisis : 2")).toBeInTheDocument();
    expect(screen.queryByLabelText("Catégorie")).toBeNull();
    await screen.findByRole("option", { name: "Gros" });
    await user.selectOptions(screen.getByLabelText("Prix à changer"), WHOLESALE);
    await user.selectOptions(screen.getByLabelText("Changement"), "amount_down");
    await user.type(screen.getByLabelText("Montant (DA)"), "5");
    await user.click(screen.getByRole("button", { name: "Voir les nouveaux prix" }));
    await screen.findByRole("row", { name: /Lait Candia 1 L/ });
    await user.type(screen.getByLabelText("Montant (DA)"), "0");
    expect(screen.queryByRole("row", { name: /Lait Candia 1 L/ })).toBeNull();
    expect(screen.getByRole("button", { name: "Appliquer" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Voir les nouveaux prix" }));
    await screen.findByRole("row", { name: /Lait Candia 1 L/ });
    await user.click(screen.getByRole("button", { name: "Appliquer" }));
    await waitFor(() => expect(applied).toHaveLength(1));
    expect(applied[0]).toMatchObject({
      productIds: ["p-milk", "p-oil"],
      priceLevelId: WHOLESALE,
      change: { kind: "amount", value: -5_000 },
      roundTo: 0,
    });
  });
});

describe("product photo", () => {
  it("shows the thumbnail from the media folder and opens the Rust file dialog", async () => {
    const picks: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      app_data_dir: () => "C:\\Users\\a\\AppData\\Roaming\\dev.dzbuild.pos",
      products_pick_image: (args) => {
        picks.push(args);
        return null;
      },
    });
    mockConvertFileSrc("windows");
    const image = "products/01923f4e-8a1b-7c2d-9e3f-4a5b6c7d8e9f-1790593200000.jpg";
    const user = userEvent.setup();
    renderWithProviders(<ProductPhoto product={{ ...productFixture, image }} editable />);
    const img = await screen.findByRole("img", { name: "Photo de Lait Candia 1 L" });
    expect(img).toHaveAttribute(
      "src",
      mediaUrl("C:\\Users\\a\\AppData\\Roaming\\dev.dzbuild.pos", image, true),
    );
    await user.click(screen.getByRole("button", { name: "Choisir une photo" }));
    await waitFor(() => expect(picks).toEqual([{ id: "p-milk" }]));
  });
});

describe("product extras", () => {
  it("saves the brand, the floor price, the usual supplier and the serial tracking", async () => {
    const saved: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      products_extras_get: () => ({ ...lotTrackedExtrasFixture, productId: "p-milk" }),
      suppliers_search: () => [cevital],
      products_extras_set: (args) => {
        saved.push(args);
        return lotTrackedExtrasFixture;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ProductExtrasPanel product={productFixture} editable />);
    await user.type(await screen.findByLabelText(/^Marque/), "Candia");
    // The floor price is a price: it opens once the session says `products.price`.
    await waitFor(() => expect(screen.getByLabelText(/Prix plancher/)).toBeEnabled());
    await user.type(screen.getByLabelText(/Prix plancher/), "140");
    await screen.findByRole("option", { name: "Cevital" });
    await user.selectOptions(screen.getByLabelText(/Fournisseur habituel/), "s-cevital");
    expect(screen.getByRole("checkbox", { name: /Suivi par lots/ })).toBeChecked();
    await user.click(screen.getByRole("checkbox", { name: "Suivi par numéros de série" }));
    await user.click(screen.getByRole("button", { name: "Enregistrer les compléments" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toEqual({
      id: "p-milk",
      input: {
        brand: "Candia",
        supplierRef: null,
        altRefs: null,
        tags: null,
        attributes: {},
        minPrice: 14_000,
        trackLots: true,
        trackSerials: true,
        preferredSupplierId: "s-cevital",
      },
    });
  });

  it("replaces the quantity breaks of every price level", async () => {
    const saved: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      catalog_price_levels_list: () => priceLevelsFixture,
      products_breaks_list: () => [{ id: "b-1", priceLevelId: RETAIL, minQty: 6_000, price: 14_000 }],
      products_breaks_replace: (args) => {
        saved.push(args);
        return [];
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<PriceBreaksEditor product={productFixture} />);
    expect(await screen.findByLabelText("À partir de (ligne 1)")).toHaveValue("6");
    await user.click(await screen.findByRole("button", { name: "Ajouter un prix par quantité" }));
    await user.selectOptions(screen.getByLabelText("Niveau de prix (ligne 2)"), WHOLESALE);
    await user.type(screen.getByLabelText("À partir de (ligne 2)"), "12");
    await user.type(screen.getByLabelText("Prix unitaire (ligne 2)"), "130");
    await user.click(screen.getByRole("button", { name: "Enregistrer les prix par quantité" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toEqual({
      id: "p-milk",
      breaks: [
        { priceLevelId: RETAIL, minQty: 6_000, price: 14_000 },
        { priceLevelId: WHOLESALE, minQty: 12_000, price: 13_000 },
      ],
    });
  });

  it("lets a price keeper without products.edit change the quantity breaks", async () => {
    const saved: unknown[] = [];
    const priceKeeper = {
      ...managerSession,
      permissions: managerSession.permissions.filter((p) => p !== "products.edit"),
    };
    mockCommands({
      auth_session: () => priceKeeper,
      catalog_price_levels_list: () => priceLevelsFixture,
      products_breaks_list: () => [{ id: "b-1", priceLevelId: RETAIL, minQty: 6_000, price: 14_000 }],
      products_breaks_replace: (args) => {
        saved.push(args);
        return [];
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<PriceBreaksEditor product={productFixture} />);
    const save = await screen.findByRole("button", { name: "Enregistrer les prix par quantité" });
    const price = screen.getByLabelText("Prix unitaire (ligne 1)");
    expect(price).toBeEnabled();
    await user.clear(price);
    await user.type(price, "135");
    await user.click(save);
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toEqual({
      id: "p-milk",
      breaks: [{ priceLevelId: RETAIL, minQty: 6_000, price: 13_500 }],
    });
  });

  it("shows the quantity breaks read-only without products.price", async () => {
    mockCommands({
      auth_session: () => storekeeperSession,
      catalog_price_levels_list: () => priceLevelsFixture,
      products_breaks_list: () => [{ id: "b-1", priceLevelId: RETAIL, minQty: 6_000, price: 14_000 }],
    });
    const { client } = renderWithProviders(<PriceBreaksEditor product={productFixture} />);
    // The gate reads the session: judge it only once the session has loaded.
    await waitFor(() => expect(client.getQueryData(qk.session)).toEqual(storekeeperSession));
    expect(await screen.findByLabelText("Prix unitaire (ligne 1)")).toBeDisabled();
    expect(screen.getByLabelText("À partir de (ligne 1)")).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Ajouter un prix par quantité" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Enregistrer les prix par quantité" })).toBeNull();
  });

  it("generates the variants of a model from its axes", async () => {
    const saved: unknown[] = [];
    const model: Product = { ...productFixture, id: "p-shirt", kind: "variant_parent", nameFr: "T-shirt" };
    mockCommands({
      auth_session: () => managerSession,
      products_variants_generate: (args) => {
        saved.push(args);
        return [productFixture];
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<VariantsDialog product={model} onClose={() => {}} />);
    await user.type(screen.getByLabelText("Critère 1"), "Taille");
    await user.type(screen.getByLabelText("Valeurs du critère 1"), "S, M, L");
    await user.click(screen.getByRole("button", { name: "Ajouter un critère" }));
    await user.type(screen.getByLabelText("Critère 2"), "Couleur");
    await user.type(screen.getByLabelText("Valeurs du critère 2"), "Rouge، Bleu");
    expect(screen.getByText("Combinaisons : 6")).toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: /code-barres interne/ }));
    await user.click(screen.getByRole("button", { name: "Créer les déclinaisons" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toEqual({
      parentId: "p-shirt",
      input: {
        axes: [
          { name: "Taille", values: ["S", "M", "L"] },
          { name: "Couleur", values: ["Rouge", "Bleu"] },
        ],
        sellPrice: null,
        costPrice: null,
        internalBarcodes: true,
      },
    });
  });

  it("edits the selected products at once, leaving the other fields alone", async () => {
    const saved: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      catalog_categories_list: () => categoriesFixture,
      catalog_tax_rates_list: () => taxRatesFixture,
      products_bulk_edit: (args) => {
        saved.push(args.edits);
        return 2;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<BulkEditDialog productIds={["p-milk", "p-oil"]} onClose={() => {}} />);
    const save = screen.getByRole("button", { name: "Enregistrer les modifications" });
    expect(save).toBeDisabled();
    await screen.findByRole("option", { name: "Épicerie" });
    await user.selectOptions(screen.getByLabelText("Catégorie"), "c-grocery");
    await user.selectOptions(screen.getByLabelText("Disponible à la caisse"), "no");
    await user.click(save);
    await waitFor(() => expect(saved).toHaveLength(1));
    const unchanged = { sellPrice: null, costPrice: null, minStock: null, taxRateId: null };
    expect(saved[0]).toEqual([
      { productId: "p-milk", ...unchanged, categoryId: "c-grocery", availableAtTill: false },
      { productId: "p-oil", ...unchanged, categoryId: "c-grocery", availableAtTill: false },
    ]);
  });

  it("shows the backend's margin and the price for a target margin", async () => {
    const inputs: PriceHelpInput[] = [];
    const used: number[] = [];
    mockCommands({
      products_price_help: (args) => {
        const input = args.input as PriceHelpInput;
        inputs.push(input);
        return {
          priceHt: 12_000,
          margin: 2_000,
          marginBp: 1_667,
          markupBp: 2_000,
          belowCost: false,
          priceForTarget: input.targetMarginBp === 2_500 ? 15_867 : null,
        };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(
      <PriceHelper cost={10_000} price={14_280} taxRateId="tr-19" onUsePrice={(p) => used.push(p)} />,
    );
    const helper = await screen.findByRole("group", { name: "Aide au prix" });
    expect(await within(helper).findByText("20,00 DA")).toBeInTheDocument();
    await user.type(within(helper).getByLabelText("Marge visée (%)"), "25");
    await user.click(await within(helper).findByRole("button", { name: "Utiliser ce prix" }));
    expect(used).toEqual([15_867]);
    expect(inputs.at(-1)).toEqual({ cost: 10_000, price: 14_280, targetMarginBp: 2_500, taxRateId: "tr-19" });
  });
});
