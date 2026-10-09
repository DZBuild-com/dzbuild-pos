import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ArchiveRow, DocState, Unit } from "@/lib/ipc/types";
import { ownerTile } from "@/test/fixtures";
import { productFixture } from "@/test/fixtures-m1";
import { locationsFixture } from "@/test/fixtures-m2";
import { lanStandaloneFixture } from "@/test/fixtures-m5";
import { packState, round, sessionWith } from "@/test/fixtures-m8";
import { missingKeys, mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { BlInvoiceCard } from "./BlInvoiceCard";
import { PackDialog } from "./PackDialog";
import { ProductWholesale } from "./ProductWholesale";
import { RoundPage } from "./RoundPage";
import { RoundsScreen } from "./RoundsScreen";

// The setup file unmounts each test before clearing the IPC mocks (the LAN status listener of the
// document settings card unsubscribes through them).

const manager = sessionWith([
  "documents.delivery",
  "credit.collect",
  "customers.view",
  "products.view",
  "products.edit",
  "products.price",
  "stock.view",
  "stock.transfer",
  "settings.company",
]);
const rounds = packState({ modules: ["wholesale", "delivery_rounds"] });

const carton: Unit = {
  id: "00000000-0000-7000-8000-000000000108",
  code: "carton",
  nameAr: "كرتونة",
  nameFr: "Carton",
  nameEn: "Carton",
  decimals: 0,
  archived: false,
};

const van = {
  id: "loc-van-1",
  code: "VAN-1",
  nameAr: "الشاحنة 1",
  nameFr: "Camion 1",
  nameEn: "Van 1",
  kind: "storage" as const,
  isDefault: false,
  archived: false,
};

/** A validated document of the archive in `state`. */
function archived(
  id: string,
  number: string,
  partyName: string,
  state: DocState,
  totalDue = 500_000,
): ArchiveRow {
  return {
    id,
    docType: number.startsWith("BL") ? "delivery_note" : "invoice",
    number,
    status: "validated",
    businessDate: "2026-10-02",
    sortAt: Date.UTC(2026, 9, 2, 9),
    partyName,
    totalTtc: totalDue,
    totalDue,
    origin: "direct",
    parentNumber: null,
    state,
    outputs: 0,
  };
}

describe("delivery rounds", () => {
  it("orders the stops of a planned round, then sends it off", async () => {
    const sent: unknown[] = [];
    const started: unknown[] = [];
    mockCommands({
      auth_session: () => manager,
      packs_state: () => rounds,
      rounds_get: () => round(),
      rounds_set_stops: (args) => {
        sent.push(args);
        return round();
      },
      rounds_start: (args) => {
        started.push(args.id);
        return round({ status: "out" });
      },
    });
    const user = userEvent.setup();
    await renderM8(<RoundPage id="r1" />, { session: manager });
    const stops = await screen.findByRole("list", { name: "Arrêts de la tournée" });
    expect(within(stops).getAllByRole("listitem")).toHaveLength(2);
    await user.click(screen.getByRole("button", { name: "Descendre l'arrêt 1" }));
    await waitFor(() => expect(sent).toEqual([{ id: "r1", documentIds: ["d2", "d1"] }]));
    await user.click(screen.getByRole("button", { name: "Retirer l'arrêt 2" }));
    await waitFor(() => expect(sent[1]).toEqual({ id: "r1", documentIds: ["d1"] }));
    await user.click(screen.getByRole("button", { name: "Départ" }));
    await waitFor(() => expect(started).toEqual(["r1"]));
  });

  it("adds only the documents still owed that ride no other open round", async () => {
    const sent: unknown[] = [];
    mockCommands({
      auth_session: () => manager,
      packs_state: () => rounds,
      rounds_get: () => round(),
      rounds_list: ({ filter }) =>
        (filter as { status: string }).status === "out"
          ? [{ ...round({ status: "out" }), id: "r2", stops: [{ ...round().stops[0]!, documentId: "d10" }] }]
          : [],
      documents_search: () => ({
        rows: [
          archived("d7", "BL2026-000040", "Supérette Nour", { state: "to_invoice" }),
          archived("d8", "FA2026-000020", "Épicerie Amine", { state: "paid" }),
          archived("d9", "FA2026-000021", "Alimentation Karim", {
            state: "unpaid",
            remaining: 250_000,
            dueDate: null,
            overdue: false,
          }),
          archived("d10", "BL2026-000041", "Épicerie Amine", { state: "to_invoice" }),
        ],
        next: null,
      }),
      rounds_set_stops: (args) => {
        sent.push(args);
        return round();
      },
    });
    const user = userEvent.setup();
    await renderM8(<RoundPage id="r1" />, { session: manager });
    await user.click(await screen.findByRole("button", { name: "Ajouter des BL et factures" }));
    const dialog = await screen.findByRole("dialog", { name: "Ajouter des BL et factures" });
    await within(dialog).findByText("BL2026-000040");
    expect(within(dialog).getByText("FA2026-000021")).toBeInTheDocument();
    expect(within(dialog).queryByText("FA2026-000020")).toBeNull();
    expect(within(dialog).queryByText("BL2026-000041")).toBeNull();
    await user.click(within(dialog).getByRole("checkbox", { name: "BL2026-000040" }));
    await user.click(within(dialog).getByRole("button", { name: "Ajouter à la tournée" }));
    await waitFor(() => expect(sent).toEqual([{ id: "r1", documentIds: ["d1", "d2", "d7"] }]));
  });

  it("plans a round with a user as its driver and a van", async () => {
    const created: unknown[] = [];
    mockCommands({
      auth_session: () => manager,
      packs_state: () => rounds,
      rounds_list: () => [],
      auth_list_users: () => [ownerTile, { ...ownerTile, id: "u-rachid", displayName: "Rachid" }],
      stock_locations: () => [...locationsFixture, van],
      rounds_create: (args) => {
        created.push(args);
        return round();
      },
      rounds_get: () => round(),
    });
    const user = userEvent.setup();
    await renderM8(<RoundsScreen />, { session: manager });
    expect(
      await screen.findByText(
        "Aucune tournée sur cette période : préparez-en une avec « Nouvelle tournée ».",
      ),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Nouvelle tournée" }));
    const dialog = await screen.findByRole("dialog", { name: "Nouvelle tournée" });
    await user.selectOptions(within(dialog).getByLabelText("Chauffeur"), "u-rachid");
    await user.type(within(dialog).getByLabelText("Immatriculation (facultatif)"), "01234-116-16");
    await waitFor(() => expect(within(dialog).getByRole("option", { name: "Camion 1" })).toBeInTheDocument());
    // the shop's default location and the damaged goods are no van
    expect(within(dialog).queryByRole("option", { name: "Magasin" })).toBeNull();
    expect(within(dialog).queryByRole("option", { name: "Marchandise abîmée" })).toBeNull();
    await user.selectOptions(within(dialog).getByLabelText("Camion (stock) (facultatif)"), "loc-van-1");
    await user.click(within(dialog).getByRole("button", { name: "Créer la tournée" }));
    await waitFor(() =>
      expect(created[0]).toMatchObject({
        input: {
          driverName: "Rachid",
          driverUserId: "u-rachid",
          vehiclePlate: "01234-116-16",
          vanLocationId: "loc-van-1",
        },
      }),
    );
    expect(await screen.findByRole("heading", { level: 1, name: "TR2026-000004" })).toBeInTheDocument();
  });

  it("unloads a closed round's van while it still holds stock", async () => {
    const unloaded: unknown[] = [];
    mockCommands({
      auth_session: () => manager,
      packs_state: () => rounds,
      rounds_get: () => round({ status: "closed", collectedFirst: 500_000 }),
      stock_locations: () => [...locationsFixture, van],
      stock_levels_view: () => [
        {
          productId: "p-flour",
          reference: null,
          nameAr: "",
          nameFr: "Farine 25 kg",
          nameEn: "",
          unitCode: "pc",
          locationId: "loc-van-1",
          qty: 3_000,
          minQty: 0,
          costPrice: 0,
          costValue: 0,
          sellPrice: 250_000,
          retailValue: 750_000,
          archived: false,
        },
      ],
      rounds_unload: (args) => {
        unloaded.push(args.id);
        return null;
      },
    });
    const user = userEvent.setup();
    await renderM8(<RoundPage id="r1" />, { session: manager });
    // read only: the receipt of the cash collected
    expect(await screen.findByText("RV2026-C1-000007")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Clôturer la tournée" })).toBeNull();
    await user.click(await screen.findByRole("button", { name: "Décharger le camion" }));
    await waitFor(() => expect(unloaded).toEqual(["r1"]));
  });

  it("reads in Arabic, right to left, with every text translated", async () => {
    missingKeys.clear();
    mockCommands({
      auth_session: () => ({ ...manager, lang: "ar" }),
      packs_state: () => rounds,
      rounds_get: () => round({ status: "out", collectedFirst: 500_000 }),
    });
    await renderM8(<RoundPage id="r1" />, { lang: "ar" });
    expect(await screen.findByRole("button", { name: "المحطة 1 — Épicerie Amine" })).toBeInTheDocument();
    expect(document.documentElement.dir).toBe("rtl");
    expect([...missingKeys]).toEqual([]);
  });
});

describe("wholesale catalogue and settings", () => {
  it("prefills a pack's names in the three languages and creates it", async () => {
    const createdPacks: unknown[] = [];
    mockCommands({
      auth_session: () => manager,
      catalog_units_list: () => [carton],
      wholesale_pack_create: (args) => {
        createdPacks.push(args.input);
        return {
          ...productFixture,
          id: "p-milk-12",
          nameFr: "Lait Candia 1 L — carton de 12",
          kind: "composite",
        };
      },
    });
    const user = userEvent.setup();
    await renderM8(<PackDialog product={productFixture} onClose={() => undefined} />, { session: manager });
    await user.type(await screen.findByLabelText("Unités par pack"), "12");
    await waitFor(() =>
      expect(screen.getByLabelText("Nom en français")).toHaveValue("Lait Candia 1 L — carton de 12"),
    );
    expect(screen.getByLabelText("Nom en arabe")).toHaveValue("حليب كانديا 1 لتر — كرتونة من 12");
    expect(screen.getByLabelText("Nom en anglais")).toHaveValue("Candia milk 1 L — carton of 12");
    await user.type(screen.getByLabelText("Prix du pack"), "1700");
    await user.click(screen.getByRole("button", { name: "Créer le pack" }));
    await waitFor(() =>
      expect(createdPacks[0]).toEqual({
        unitProductId: "p-milk",
        unitsPerPack: 12_000,
        unitId: carton.id,
        nameAr: "حليب كانديا 1 لتر — كرتونة من 12",
        nameFr: "Lait Candia 1 L — carton de 12",
        nameEn: "Candia milk 1 L — carton of 12",
        price: 170_000,
        barcode: null,
      }),
    );
  });

  it("saves a product's minimum order when the field is left", async () => {
    const set: unknown[] = [];
    mockCommands({
      auth_session: () => manager,
      packs_state: () => packState({ modules: ["wholesale"] }),
      catalog_units_list: () => [carton],
      wholesale_min_order: () => null,
      wholesale_min_order_set: (args) => {
        set.push(args);
        return 24_000;
      },
    });
    const user = userEvent.setup();
    await renderM8(<ProductWholesale product={productFixture} session={manager} />, { session: manager });
    const field = await screen.findByLabelText("Minimum de commande");
    await waitFor(() => expect(field).toBeEnabled());
    expect(screen.getByRole("button", { name: "Créer un pack" })).toBeInTheDocument();
    await user.type(field, "24");
    await user.tab();
    await waitFor(() => expect(set).toEqual([{ productId: "p-milk", minOrderQty: 24_000 }]));
  });

  it("will not turn the BL-as-invoice mode on without the decision's number and date", async () => {
    const set: unknown[] = [];
    mockCommands({
      auth_session: () => manager,
      packs_state: () => packState({ modules: ["wholesale"] }),
      settings_get: () => null,
      settings_set: (args) => {
        set.push(args);
        return args.value;
      },
      lan_status: () => lanStandaloneFixture,
    });
    const user = userEvent.setup();
    await renderM8(<BlInvoiceCard />, { session: manager });
    await user.click(await screen.findByRole("checkbox", { name: "Mes bons de livraison valent facture" }));
    await user.click(screen.getByRole("button", { name: "Enregistrer" }));
    expect(
      screen.getByText("Saisissez le numéro et la date de la décision pour activer ce mode."),
    ).toBeInTheDocument();
    expect(set).toEqual([]);
    await user.type(screen.getByLabelText("N° de la décision"), "117/2026");
    await user.type(screen.getByLabelText("Date de la décision"), "2026-05-12");
    expect(screen.getByTestId("bl-invoice-line")).toHaveTextContent(
      "BL valant facture — autorisation n° 117/2026 du 2026-05-12",
    );
    await user.click(screen.getByRole("button", { name: "Enregistrer" }));
    await waitFor(() =>
      expect(set).toEqual([
        { key: "documents.bl_as_invoice", value: { number: "117/2026", date: "2026-05-12" } },
      ]),
    );
  });
});
