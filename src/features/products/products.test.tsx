import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProductsScreen } from "@/features/products/ProductsScreen";
import { setLanguage } from "@/lib/i18n";
import { ownerSession, storekeeperSession } from "@/test/fixtures";
import {
  categoriesFixture,
  priceLevelsFixture,
  productFixture,
  productHitsFixture,
  stockLevelsFixture,
  taxRatesFixture,
  unitsFixture,
} from "@/test/fixtures-m1";
import { scaleItemsFixture } from "@/test/fixtures-m4";
import { clientStatusFixture, lanTillStatusFixture } from "@/test/fixtures-m5";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";

function mockCatalogue(extra: Record<string, Handler> = {}) {
  return mockCommands({
    catalog_categories_list: () => categoriesFixture,
    catalog_units_list: () => unitsFixture,
    catalog_tax_rates_list: () => taxRatesFixture,
    catalog_price_levels_list: () => priceLevelsFixture,
    products_grid: () => productHitsFixture,
    products_catalog: () => productHitsFixture,
    products_search: () => productHitsFixture.slice(0, 1),
    products_get: () => productFixture,
    stock_levels: () => stockLevelsFixture,
    stock_movements: () => [],
    ...extra,
  });
}

describe("ProductsScreen", () => {
  beforeEach(() => {
    // jsdom has no layout: TanStack Virtual measures the table's scroller (offsetWidth/offsetHeight)
    // on mount, replacing `initialRect`, and would render no row at 0×0.
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1000);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
  });

  it("lists every product a page at a time, archived ones on demand, with their state", async () => {
    const queries: unknown[] = [];
    const page = Array.from({ length: 100 }, (_, i) => ({
      ...productHitsFixture[0]!,
      id: `p-${String(i).padStart(3, "0")}`,
      nameFr: `Article ${String(i).padStart(3, "0")}`,
    }));
    mockCatalogue({
      products_catalog: (args) => {
        queries.push(args);
        const query = args.query as { archived: string; after: { id: string } | null };
        if (query.archived === "archived") return [{ ...productHitsFixture[1]!, archived: true }];
        if (query.after)
          return [{ ...productHitsFixture[0]!, id: "p-back", nameFr: "Carton vide", availableAtTill: false }];
        return page;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ProductsScreen session={ownerSession} />, { route: "/products" });
    expect(await screen.findByRole("row", { name: /Article 000/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Afficher plus" }));
    await waitFor(() => expect(queries).toHaveLength(2));
    expect(queries[1]).toMatchObject({
      query: { archived: "active", after: { nameFr: "Article 099", id: "p-099" }, limit: 100 },
    });
    await user.selectOptions(screen.getByLabelText("Afficher"), "archived");
    expect(await screen.findByText("Retiré de la vente")).toBeInTheDocument();
    expect(queries.at(-1)).toMatchObject({ query: { archived: "archived", after: null } });
  });

  it("exports the catalogue (IMX-010) for whoever sees the products", async () => {
    const saved: unknown[] = [];
    mockCatalogue({
      exports_save: (args) => {
        saved.push(args.request);
        return { path: "C:/Users/caisse/Documents/products.xlsx", rows: 2 };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ProductsScreen session={storekeeperSession} />, { route: "/products" });
    await user.click(await screen.findByRole("button", { name: "Excel (xlsx)" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({ target: { kind: "products" }, format: "xlsx", lang: "fr" });
  });

  it("lists products and creates one with a barcode", async () => {
    const created: unknown[] = [];
    mockCatalogue({
      products_create: (args) => {
        created.push(args);
        return productFixture;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ProductsScreen session={ownerSession} />, { route: "/products" });
    expect(await screen.findByRole("row", { name: /Huile Elio 5 L/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Nouveau produit" }));
    const dialog = await screen.findByRole("dialog", { name: "Nouveau produit" });
    await user.type(within(dialog).getByLabelText("Nom en français"), "Sucre Cevital 1 kg");
    await user.type(within(dialog).getByLabelText("Nom en arabe"), "سكر سيفيتال 1 كغ");
    await user.type(within(dialog).getByLabelText("Prix de vente"), "95");
    await user.type(within(dialog).getByLabelText("Code-barres"), "6130000000031");
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer le produit" }));
    await waitFor(() => expect(created).toHaveLength(1));
    expect(created[0]).toMatchObject({
      input: {
        nameFr: "Sucre Cevital 1 kg",
        nameAr: "سكر سيفيتال 1 كغ",
        sellPrice: 9_500,
        kind: "simple",
        unitId: unitsFixture[0]?.id,
        taxRateId: "tr-19",
        barcodes: [{ barcode: "6130000000031", packQty: 1_000, label: null }],
        trackStock: true,
      },
    });
  });

  it("refuses a product without a name or a price", async () => {
    mockCatalogue();
    const user = userEvent.setup();
    renderWithProviders(<ProductsScreen session={ownerSession} />, { route: "/products" });
    await user.click(await screen.findByRole("button", { name: "Nouveau produit" }));
    const dialog = await screen.findByRole("dialog", { name: "Nouveau produit" });
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer le produit" }));
    expect(await within(dialog).findByText("Remplissez au moins un nom.")).toBeInTheDocument();
    expect(within(dialog).getByText("Saisissez un montant comme 1 000 000,00.")).toBeInTheDocument();
  });

  it("explains a level price it cannot read instead of refusing silently", async () => {
    const created: unknown[] = [];
    mockCatalogue({
      products_create: (args) => {
        created.push(args);
        return productFixture;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ProductsScreen session={ownerSession} />, { route: "/products" });
    await user.click(await screen.findByRole("button", { name: "Nouveau produit" }));
    const dialog = await screen.findByRole("dialog", { name: "Nouveau produit" });
    await user.type(within(dialog).getByLabelText("Nom en français"), "Sucre Cevital 1 kg");
    await user.type(within(dialog).getByLabelText("Prix de vente"), "95");
    await user.type(within(dialog).getByLabelText(/Prix Gros/), ",5");
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer le produit" }));
    expect(await within(dialog).findByText("Saisissez un montant comme 1 000 000,00.")).toBeInTheDocument();
    expect(created).toEqual([]);
  });

  it("adjusts stock with a reason from the product", async () => {
    const adjustments: unknown[] = [];
    mockCatalogue({
      stock_adjust: (args) => {
        adjustments.push(args);
        return { id: "m-1" };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ProductsScreen session={ownerSession} />, { route: "/products" });
    await user.click(await screen.findByRole("row", { name: /Lait Candia 1 L/ }));
    const dialog = await screen.findByRole("dialog", { name: "Lait Candia 1 L" });
    await user.click(within(dialog).getByRole("tab", { name: "Stock" }));
    expect(await within(dialog).findByText("24")).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText("Quantité à ajouter ou retirer"), "-2");
    await user.selectOptions(within(dialog).getByLabelText("Raison"), "damage");
    await user.click(within(dialog).getByRole("button", { name: "Corriger le stock" }));
    await waitFor(() =>
      expect(adjustments).toEqual([
        {
          input: {
            productId: "p-milk",
            locationId: null,
            qtyDelta: -2_000,
            reason: "damage",
            unitCost: null,
            note: null,
          },
        },
      ]),
    );
  });

  it("hides costs from a user without reports.profit", async () => {
    mockCatalogue();
    const user = userEvent.setup();
    const storekeeper = {
      ...ownerSession,
      isOwner: false,
      permissions: ownerSession.permissions.filter((p) => p !== "reports.profit"),
    };
    renderWithProviders(<ProductsScreen session={storekeeper} />, { route: "/products" });
    await user.click(await screen.findByRole("row", { name: /Lait Candia 1 L/ }));
    const dialog = await screen.findByRole("dialog", { name: "Lait Candia 1 L" });
    expect(within(dialog).queryByLabelText("Prix d'achat (CMUP)")).toBeNull();
  });

  it("shows the prices read-only to an editor without products.price", async () => {
    // The backend keeps the stored prices for such an editor (`products_update`): the form must
    // not let them type a price that would be dropped.
    mockCatalogue();
    const user = userEvent.setup();
    const storekeeper = {
      ...ownerSession,
      isOwner: false,
      permissions: ownerSession.permissions.filter((p) => p !== "products.price"),
    };
    renderWithProviders(<ProductsScreen session={storekeeper} />, { route: "/products" });
    await user.click(await screen.findByRole("row", { name: /Lait Candia 1 L/ }));
    const dialog = await screen.findByRole("dialog", { name: "Lait Candia 1 L" });
    expect(within(dialog).getByLabelText("Prix de vente")).toBeDisabled();
    expect(within(dialog).getByLabelText(/Prix Gros/)).toBeDisabled();
    expect(within(dialog).getByLabelText("Nom en français")).toBeEnabled();
  });

  it("ticks products without opening them and edits the ticked ones together", async () => {
    const edits: unknown[] = [];
    mockCatalogue({
      products_bulk_edit: (args) => {
        edits.push(args.edits);
        return 2;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ProductsScreen session={ownerSession} />, { route: "/products" });
    await user.click(await screen.findByRole("checkbox", { name: "Sélectionner Lait Candia 1 L" }));
    await user.click(screen.getByRole("checkbox", { name: "Sélectionner Huile Elio 5 L" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Modifier la sélection (2)" }));
    const dialog = await screen.findByRole("dialog", { name: "Modifier la sélection" });
    await user.selectOptions(within(dialog).getByLabelText("Disponible à la caisse"), "no");
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer les modifications" }));
    await waitFor(() => expect(edits).toHaveLength(1));
    expect(edits[0]).toMatchObject([
      { productId: "p-milk", availableAtTill: false, categoryId: null },
      { productId: "p-oil", availableAtTill: false, categoryId: null },
    ]);
  });

  it("puts the helper's price for a target margin in the sale price", async () => {
    const helped: unknown[] = [];
    mockCatalogue({
      products_get: () => ({ ...productFixture, costPrice: 10_000 }),
      products_price_help: (args) => {
        const input = args.input as { targetMarginBp: number | null };
        helped.push(input);
        return {
          priceHt: 13_761,
          margin: 3_761,
          marginBp: 2_733,
          markupBp: 3_761,
          belowCost: false,
          priceForTarget: input.targetMarginBp === 2_500 ? 14_533 : null,
        };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ProductsScreen session={ownerSession} />, { route: "/products" });
    await user.click(await screen.findByRole("row", { name: /Lait Candia 1 L/ }));
    const dialog = await screen.findByRole("dialog", { name: "Lait Candia 1 L" });
    const helper = within(dialog).getByRole("group", { name: "Aide au prix" });
    await user.type(within(helper).getByLabelText("Marge visée (%)"), "25");
    await user.click(await within(helper).findByRole("button", { name: "Utiliser ce prix" }));
    expect(within(dialog).getByLabelText("Prix de vente")).toHaveValue("145,33");
    expect(helped).toContainEqual({ cost: 10_000, price: 15_000, targetMarginBp: 2_500, taxRateId: "tr-9" });
  });

  it("shows a variant model's variants and opens the variants matrix", async () => {
    const model = { ...productFixture, id: "p-shirt", kind: "variant_parent" as const, nameFr: "T-shirt" };
    mockCatalogue({
      products_get: () => model,
      products_variants: () => [{ ...productHitsFixture[0]!, id: "p-shirt-m", nameFr: "T-shirt M" }],
    });
    const user = userEvent.setup();
    renderWithProviders(<ProductsScreen session={ownerSession} />, { route: "/products" });
    await user.click(await screen.findByRole("row", { name: /Lait Candia 1 L/ }));
    const dialog = await screen.findByRole("dialog", { name: "T-shirt" });
    expect(within(dialog).getByLabelText("Type")).toHaveValue("variant_parent");
    await user.click(within(dialog).getByRole("tab", { name: "Déclinaisons" }));
    expect(await within(dialog).findByText("T-shirt M")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Créer des déclinaisons" }));
    expect(
      await screen.findByRole("dialog", { name: "Créer des déclinaisons : T-shirt" }),
    ).toBeInTheDocument();
  });

  it("edits products on a till only while the main PC answers, and never the categories", async () => {
    let status: unknown = lanTillStatusFixture;
    mockCatalogue({ lan_status: () => status });
    const { client } = renderWithProviders(<ProductsScreen session={ownerSession} />);
    expect(await screen.findByText("Les modifications passent par le poste principal.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Nouveau produit" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Catégories" })).not.toBeInTheDocument();
    status = { ...lanTillStatusFixture, client: { ...clientStatusFixture, link: "offline" } };
    await act(() => client.invalidateQueries({ queryKey: ["lan"] }));
    expect(
      await screen.findByText(
        "Le poste principal ne répond pas : les fiches se modifient sur le poste principal, ou ici à son retour.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Nouveau produit" })).not.toBeInTheDocument();
  });

  it("shows a product's scale settings read-only on a till, with the main PC's name", async () => {
    mockCatalogue({
      lan_status: () => lanTillStatusFixture,
      products_get: () => ({ ...productFixture, plu: 12 }),
      scale_items_list: () => [{ ...scaleItemsFixture[0]!, productId: productFixture.id }],
    });
    const user = userEvent.setup();
    renderWithProviders(<ProductsScreen session={ownerSession} />, { route: "/products" });
    expect(await screen.findByText("Les modifications passent par le poste principal.")).toBeInTheDocument();
    await user.click(await screen.findByRole("row", { name: /Lait Candia 1 L/ }));
    const dialog = await screen.findByRole("dialog", { name: "Lait Candia 1 L" });
    expect(await within(dialog).findByLabelText("Tare (g)")).toHaveValue("120");
    expect(within(dialog).getByLabelText("Tare (g)")).toBeDisabled();
    expect(within(dialog).getByLabelText("Touche balance")).toBeDisabled();
    expect(
      within(dialog).getByText(
        "Lecture seule sur ce poste de caisse : faites-le sur le poste principal (Supérette El Amel).",
      ),
    ).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Nom en français")).toBeEnabled();
  });

  it("reads right to left in Arabic", async () => {
    await setLanguage("ar");
    mockCatalogue();
    renderWithProviders(<ProductsScreen session={ownerSession} />, { route: "/products" });
    expect(await screen.findByRole("row", { name: /زيت إليو 5 لتر/ })).toBeInTheDocument();
    expect(document.documentElement).toHaveAttribute("dir", "rtl");
  });
});
