import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CountScreen } from "@/features/stock/CountScreen";
import { CountsTab } from "@/features/stock/CountsTab";
import { ExpiryTab } from "@/features/stock/ExpiryTab";
import { LevelsTab } from "@/features/stock/LevelsTab";
import { MovementsTab } from "@/features/stock/MovementsTab";
import { SerialsTab } from "@/features/stock/SerialsTab";
import { StockScreen } from "@/features/stock/StockScreen";
import { TransferDialog } from "@/features/stock/TransferDialog";
import type { CountLineView, MovementRow, Serial, Session, SupplierHit } from "@/lib/ipc/types";
import { categoriesFixture, documentFixture, productFixture, unitsFixture } from "@/test/fixtures-m1";
import {
  countFixture,
  countLinesFixture,
  expiryFixture,
  levelsFixture,
  locationsFixture,
  lotTrackedExtrasFixture,
  supplierFixture,
} from "@/test/fixtures-m2";
import { ownerSession, storekeeperSession } from "@/test/fixtures";
import { setBurstGuard } from "@/lib/scanner";
import { mockCommands, renderWithProviders, scan } from "@/test/render";
import { scanDigits, scannerKeys } from "@/test/scan";

beforeEach(() => {
  // jsdom has no layout: TanStack Virtual measures its scroller with offsetWidth/offsetHeight and
  // shows no row at 0×0, so every element measures 1024×600.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

const SHOP = "00000000-0000-7000-8000-000000000301";

/** A role that counts and nothing else (a helper on inventory day). */
const counterSession: Session = { ...storekeeperSession, permissions: ["stock.count"] };

/** The count's lines as the backend holds them: a scan counts its product. */
function countedLines(counted: Map<string, number>): CountLineView[] {
  return countLinesFixture.map((line) =>
    counted.has(line.productId) ? { ...line, countedQty: counted.get(line.productId) ?? null } : line,
  );
}

describe("stock counts", () => {
  it("adds one pack per scan and hides the expected quantity of a blind count", async () => {
    const scans: string[] = [];
    const counted = new Map<string, number>();
    mockCommands({
      auth_session: () => storekeeperSession,
      counts_lines: () => countedLines(counted),
      counts_scan: (args) => {
        scans.push(args.code as string);
        counted.set("p-huile", 1000);
        return { ...countLinesFixture[0], countedQty: 1000 };
      },
    });
    renderWithProviders(<CountScreen count={{ ...countFixture, blind: true }} />);
    expect(await screen.findByText("Huile 5 L")).toBeInTheDocument();
    expect(screen.queryByText("Attendu")).not.toBeInTheDocument();
    scan("6130000000015");
    await waitFor(() => expect(scans).toEqual(["6130000000015"]));
    expect(await screen.findByText("1 / 2")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Dernier scan : Huile 5 L, compté : 1 (pc).");
  });

  it("counts a scan made while the search has the focus and keeps the search as typed", async () => {
    setBurstGuard(true);
    const scans: string[] = [];
    mockCommands({
      auth_session: () => storekeeperSession,
      counts_lines: () => countLinesFixture,
      counts_scan: (args) => {
        scans.push(args.code as string);
        return { ...countLinesFixture[0], countedQty: 1000 };
      },
    });
    renderWithProviders(<CountScreen count={countFixture} />);
    const search = await screen.findByLabelText("Rechercher");
    search.focus();
    fireEvent.change(search, { target: { value: "hui" } });
    // the scanner types into the search: the window scanner counts it, the field drops the burst
    scanDigits(search, "6130000000015");
    await waitFor(() => expect(scans).toEqual(["6130000000015"]));
    expect(search).toHaveValue("hui");
    setBurstGuard(false);
  });

  it("counts the codes of a serial scanner and of a scanner sending a prefix (HW-020/022)", async () => {
    setBurstGuard(true);
    const scans: string[] = [];
    mockCommands({
      auth_session: () => storekeeperSession,
      hardware_get: () => ({
        version: 2,
        printers: [],
        receiptPrinterId: null,
        scanner: { layout: "us", prefix: "]E0" },
      }),
      counts_lines: () => countLinesFixture,
      counts_scan: (args) => {
        scans.push(args.code as string);
        return { ...countLinesFixture[0], countedQty: 1000 };
      },
    });
    renderWithProviders(<CountScreen count={countFixture} />);
    expect(await screen.findByText("Huile 5 L")).toBeInTheDocument();
    // sent again until the screen's listener is in place (it subscribes asynchronously)
    await waitFor(async () => {
      if (!scans.includes("6130000000015"))
        await act(() => emit("serial-scanned", { code: "6130000000015" }));
      expect(scans).toContain("6130000000015");
    });
    // the hub's prefix (an AIM identifier) is removed from a keyboard scan
    await waitFor(() => {
      if (!scans.includes("6130000000022")) {
        scannerKeys(document.body, [
          { code: "BracketRight", key: "]" },
          { code: "KeyE", key: "E", shiftKey: true },
          { code: "Digit0", key: "0" },
          ...[..."6130000000022"].map((d) => ({ code: `Digit${d}`, key: d })),
          { code: "Enter", key: "Enter" },
        ]);
      }
      expect(scans).toContain("6130000000022");
    });
    setBurstGuard(false);
  });

  it("shows a blind count's expected quantities to whoever may adjust the stock, and saves a typed count", async () => {
    const sets: unknown[] = [];
    mockCommands({
      auth_session: () => storekeeperSession,
      counts_lines: () => countLinesFixture,
      counts_set: (args) => {
        sets.push(args);
        return { ...countLinesFixture[1], countedQty: 8_000 };
      },
    });
    renderWithProviders(<CountScreen count={{ ...countFixture, blind: true }} />);
    fireEvent.click(await screen.findByRole("button", { name: "Afficher les quantités attendues" }));
    expect(await screen.findByText("Attendu")).toBeInTheDocument();
    expect(screen.getByText("Écart")).toBeInTheDocument();
    const field = screen.getByLabelText("Compté — Lait UHT 1 L");
    fireEvent.change(field, { target: { value: "8" } });
    fireEvent.blur(field);
    await waitFor(() => expect(sets).toEqual([{ id: "c-1", productId: "p-lait", qty: 8_000, mode: "set" }]));
  });

  it("counts a lot-tracked medicine lot by lot and adds a lot found on the shelf (STK-004)", async () => {
    const sets: unknown[] = [];
    const lot = (lotId: string, lotNumber: string, expiryDate: string, expectedQty: number) => ({
      productId: "p-huile",
      lotId,
      lotNumber,
      expiryDate,
      expectedQty,
      countedQty: null,
      variance: null,
    });
    mockCommands({
      auth_session: () => storekeeperSession,
      counts_lines: () => countLinesFixture,
      counts_lot_lines: () => [
        lot("l-a", "L-A", "2027-01-31", 6_000),
        lot("l-b", "L-B", "2027-06-30", 4_000),
      ],
      counts_set_lot: (args) => {
        sets.push(args);
        return { ...countLinesFixture[0], countedQty: 5_000 };
      },
    });
    renderWithProviders(<CountScreen count={countFixture} />);
    // the oil is counted by its lots, the milk as before
    fireEvent.click(await screen.findByRole("button", { name: "Compter les lots de Huile 5 L" }));
    const dialog = await screen.findByRole("dialog", { name: "Lots — Huile 5 L" });
    expect(screen.getByLabelText("Compté — Lait UHT 1 L")).toBeInTheDocument();
    expect(within(dialog).getByText("L-B")).toBeInTheDocument();
    const field = within(dialog).getByLabelText("Compté du lot L-A");
    fireEvent.change(field, { target: { value: "5" } });
    fireEvent.blur(field);
    await waitFor(() =>
      expect(sets).toEqual([{ id: "c-1", productId: "p-huile", lotNumber: "L-A", qty: 5_000, mode: "set" }]),
    );
    fireEvent.change(within(dialog).getByLabelText("N° de lot"), { target: { value: "L-C" } });
    fireEvent.change(within(dialog).getByLabelText("Compté"), { target: { value: "2" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Ajouter le lot" }));
    await waitFor(() =>
      expect(sets.at(-1)).toEqual({
        id: "c-1",
        productId: "p-huile",
        lotNumber: "L-C",
        qty: 2_000,
        mode: "set",
      }),
    );
  });

  it("never offers a counter without stock.adjust the expected quantities of a blind count", async () => {
    mockCommands({
      auth_session: () => counterSession,
      counts_lines: () => countLinesFixture.map((l) => ({ ...l, expectedQty: null })),
    });
    renderWithProviders(<CountScreen count={{ ...countFixture, blind: true }} />);
    // the close button shows once the session is read
    expect(await screen.findByRole("button", { name: "Clôturer l'inventaire" })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Afficher les quantités attendues" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Attendu")).not.toBeInTheDocument();
  });

  it("says which scanned code the backend does not know", async () => {
    mockCommands({
      auth_session: () => storekeeperSession,
      counts_lines: () => countLinesFixture,
      counts_scan: () => Promise.reject({ code: "product_not_found" }),
    });
    renderWithProviders(<CountScreen count={countFixture} />);
    expect(await screen.findByText("Huile 5 L")).toBeInTheDocument();
    scan("6139999999999");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Code 6139999999999 : Produit introuvable. Vérifiez le code ou cherchez par nom.",
    );
  });

  it("closes a full count with the uncounted products set to zero and shows the backend's variance", async () => {
    const closes: unknown[] = [];
    const onBack = vi.fn();
    mockCommands({
      auth_session: () => ownerSession,
      counts_lines: () => countLinesFixture,
      counts_close: (args) => {
        closes.push(args);
        return { count: { ...countFixture, status: "closed" }, adjusted: 2, varianceValue: -125_000 };
      },
    });
    renderWithProviders(<CountScreen count={countFixture} onBack={onBack} />);
    fireEvent.click(await screen.findByRole("button", { name: "Clôturer l'inventaire" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Produits non comptés : 2")).toBeInTheDocument();
    expect(within(dialog).getByRole("radio", { name: /Les mettre à zéro/ })).toBeChecked();
    fireEvent.click(within(dialog).getByRole("button", { name: "Clôturer et corriger le stock" }));
    await waitFor(() => expect(closes).toEqual([{ id: "c-1", uncounted: "zero" }]));
    expect(await screen.findByText("Produits corrigés : 2")).toBeInTheDocument();
    expect(screen.getByText(/-1\s250,00/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Terminé" }));
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it("keeps the stock of the uncounted products of a partial count by default", async () => {
    mockCommands({ auth_session: () => ownerSession, counts_lines: () => countLinesFixture });
    renderWithProviders(<CountScreen count={{ ...countFixture, scope: "products" }} />);
    fireEvent.click(await screen.findByRole("button", { name: "Clôturer l'inventaire" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("radio", { name: /Garder leur stock/ })).toBeChecked();
  });

  it("lists the counts and starts a blind count of a category", async () => {
    const started: unknown[] = [];
    mockCommands({
      auth_session: () => storekeeperSession,
      auth_list_users: () => [],
      counts_list: () => [countFixture],
      stock_locations: () => locationsFixture,
      catalog_categories_list: () => categoriesFixture,
      counts_start: (args) => {
        started.push(args.input);
        return { ...countFixture, id: "c-2", scope: "category", categoryId: "c-dairy", blind: true };
      },
      counts_lines: () => countLinesFixture,
    });
    renderWithProviders(<CountsTab />);
    const row = await screen.findByRole("row", { name: /En cours/ });
    expect(await within(row).findByText("Magasin")).toBeInTheDocument();
    expect(within(row).getByText("Inventaire complet")).toBeInTheDocument();
    expect(within(row).getByText("0 / 2")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Nouvel inventaire" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("radio", { name: /Une catégorie/ }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Commencer l'inventaire" }));
    expect(await within(dialog).findByText("Remplissez ce champ.")).toBeInTheDocument();
    expect(started).toEqual([]);
    fireEvent.change(within(dialog).getByLabelText("Catégorie"), { target: { value: "c-dairy" } });
    fireEvent.click(within(dialog).getByRole("checkbox", { name: /Inventaire à l'aveugle/ }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Commencer l'inventaire" }));
    await waitFor(() =>
      expect(started).toEqual([
        {
          locationId: SHOP,
          scope: "category",
          categoryId: "c-dairy",
          productIds: [],
          blind: true,
          note: null,
        },
      ]),
    );
    expect(await screen.findByRole("heading", { name: /— Magasin$/ })).toBeInTheDocument();
    expect(screen.getByText("Catégorie : Crèmerie")).toBeInTheDocument();
    expect(screen.getByText("À l'aveugle")).toBeInTheDocument();
  });
});

describe("stock transfers", () => {
  it("moves a chosen lot from the shop to the warehouse and shows the BT number", async () => {
    const sent: unknown[] = [];
    const lot = (lotNumber: string, locationId: string, qty: number) => ({
      lot: {
        id: `lot-${lotNumber}`,
        productId: "p-milk",
        lotNumber,
        expiryDate: "2026-10-05",
        supplierId: null,
        createdAt: 0,
      },
      locationId,
      qty,
    });
    mockCommands({
      auth_session: () => storekeeperSession,
      stock_locations: () => locationsFixture,
      catalog_units_list: () => unitsFixture,
      products_find_by_code: () => ({ product: productFixture, packQty: 1_000 }),
      products_extras_get: () => lotTrackedExtrasFixture,
      lots_of_product: () => [lot("L2609", SHOP, 24_000), lot("L3009", "loc-depot", 12_000)],
      stock_transfer: (args) => {
        sent.push(args.input);
        return {
          document: { ...documentFixture, docType: "stock_transfer", number: "BT2026-000001" },
          fromLocationId: SHOP,
          toLocationId: "loc-depot",
        };
      },
    });
    renderWithProviders(<TransferDialog onClose={() => {}} />);
    await waitFor(() => expect(screen.getByLabelText("Vers")).toHaveValue("loc-depot"));
    const picker = screen.getByLabelText("Ajouter un produit");
    fireEvent.change(picker, { target: { value: "6130000000017" } });
    fireEvent.keyDown(picker, { key: "Enter" });
    const lotField = await screen.findByLabelText("Lot — Lait Candia 1 L");
    // only the lots held at the origin are offered
    expect(await within(lotField).findByRole("option", { name: /L2609/ })).toBeInTheDocument();
    expect(within(lotField).queryByRole("option", { name: /L3009/ })).not.toBeInTheDocument();
    fireEvent.change(lotField, { target: { value: "L2609" } });
    fireEvent.change(screen.getByLabelText("Quantité — Lait Candia 1 L"), { target: { value: "12" } });
    fireEvent.click(screen.getByRole("button", { name: "Valider le transfert" }));
    await waitFor(() =>
      expect(sent).toEqual([
        {
          fromLocationId: SHOP,
          toLocationId: "loc-depot",
          lines: [{ productId: "p-milk", qty: 12_000, lotNumber: "L2609", serials: [] }],
          driverName: null,
          vehicle: null,
          note: null,
        },
      ]),
    );
    const done = await screen.findByRole("dialog", { name: "Transfert BT2026-000001 validé" });
    expect(within(done).getByText(/De Magasin vers Dépôt/)).toBeInTheDocument();
  });
});

describe("expiry", () => {
  it("lists the expiring lots and removes an expired one with the reason expiry", async () => {
    const adjusted: unknown[] = [];
    mockCommands({
      auth_session: () => storekeeperSession,
      stock_locations: () => locationsFixture,
      lots_expiry: () => expiryFixture,
      stock_adjust_batch: (args) => {
        adjusted.push(args.input);
        return [];
      },
    });
    renderWithProviders(<ExpiryTab />);
    const soon = await screen.findByRole("row", { name: /Lait UHT 1 L/ });
    expect(within(soon).getByText("Dans 5 jours")).toBeInTheDocument();
    expect(within(soon).queryByRole("button")).not.toBeInTheDocument();
    const expired = screen.getByRole("row", { name: /Yaourt nature/ });
    expect(within(expired).getByText("Depuis 3 jours")).toBeInTheDocument();
    expect(within(expired).getByText("Périmé")).toBeInTheDocument();
    fireEvent.click(
      await within(expired).findByRole("button", { name: "Retirer (périmé) — Yaourt nature, lot L0901" }),
    );
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Retirer (périmé)" }));
    await waitFor(() =>
      expect(adjusted).toEqual([
        {
          locationId: SHOP,
          reason: "expiry",
          lines: [{ productId: "p-yaourt", qtyDelta: -6_000, unitCost: null }],
          note: "Lot L0901 périmé",
        },
      ]),
    );
  });
});

describe("stock levels", () => {
  it("hides the value at cost from the storekeeper and filters the products below their minimum", async () => {
    const queries: unknown[] = [];
    mockCommands({
      auth_session: () => storekeeperSession,
      stock_locations: () => locationsFixture,
      catalog_categories_list: () => categoriesFixture,
      stock_levels_view: (args) => {
        queries.push(args.query);
        return levelsFixture;
      },
    });
    const { unmount } = renderWithProviders(<LevelsTab />);
    const milk = await screen.findByRole("row", { name: /Lait UHT 1 L/ });
    expect(within(milk).getByRole("img", { name: "Sous le minimum" })).toBeInTheDocument();
    expect(await screen.findByText("Valeur de vente")).toBeInTheDocument();
    // the minimums button appears once the session is read (stock.adjust)
    expect(await within(milk).findByRole("button", { name: "Minimums — Lait UHT 1 L" })).toBeInTheDocument();
    expect(screen.queryByText("Valeur au CMUP")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: "Sous le minimum seulement" }));
    await waitFor(() => expect(queries.at(-1)).toMatchObject({ onlyBelowMin: true }));
    unmount();

    mockCommands({
      auth_session: () => ownerSession,
      stock_locations: () => locationsFixture,
      catalog_categories_list: () => categoriesFixture,
      stock_levels_view: () => levelsFixture.map((r) => ({ ...r, costValue: 18_000 })),
    });
    renderWithProviders(<LevelsTab />);
    expect(await screen.findByText("Valeur au CMUP")).toBeInTheDocument();
  });

  it("says when the list stops at its limit, turns its sorting off and flags archived stock", async () => {
    const queries: Record<string, unknown>[] = [];
    const many = Array.from({ length: 2_001 }, (_, i) => ({
      ...levelsFixture[1]!,
      productId: `p-${i}`,
      nameFr: `Article ${String(i).padStart(4, "0")}`,
      archived: i === 0,
    }));
    mockCommands({
      auth_session: () => storekeeperSession,
      stock_locations: () => locationsFixture,
      catalog_categories_list: () => categoriesFixture,
      stock_levels_view: (args) => {
        queries.push(args.query as Record<string, unknown>);
        return many;
      },
    });
    renderWithProviders(<LevelsTab />);
    expect(await screen.findByText(/Seules les 2\s?000 premières lignes/)).toBeInTheDocument();
    expect(queries[0]).toMatchObject({ limit: 2_001 });
    const first = await screen.findByRole("row", { name: /Article 0000/ });
    expect(within(first).getByText("Archivé")).toBeInTheDocument();
    // a cut list keeps the backend's order: no header sorts it
    const header = screen.getByRole("columnheader", { name: "En stock" });
    expect(header).not.toHaveAttribute("aria-sort");
    expect(within(header).queryByRole("button")).not.toBeInTheDocument();
  });

  it("puts a scanned barcode in the search whatever the Windows layout", async () => {
    setBurstGuard(true);
    const queries: Record<string, unknown>[] = [];
    mockCommands({
      auth_session: () => storekeeperSession,
      stock_locations: () => locationsFixture,
      catalog_categories_list: () => categoriesFixture,
      stock_levels_view: (args) => {
        queries.push(args.query as Record<string, unknown>);
        return levelsFixture;
      },
    });
    renderWithProviders(<LevelsTab />);
    const search = await screen.findByLabelText("Rechercher");
    search.focus();
    // an AZERTY Windows types & é " ' ( - è _ ç à for the digit row the scanner presses
    const azerty: Record<string, string> = {
      "1": "&",
      "2": "é",
      "3": '"',
      "4": "'",
      "5": "(",
      "6": "-",
      "7": "è",
      "8": "_",
      "9": "ç",
      "0": "à",
    };
    scanDigits(search, "6130000000017", (d) => azerty[d] ?? d);
    expect(search).toHaveValue("6130000000017");
    await waitFor(() => expect(queries.at(-1)).toMatchObject({ text: "6130000000017" }));
    setBurstGuard(false);
  });

  it("saves a product's minimum per location", async () => {
    const saved: unknown[] = [];
    mockCommands({
      auth_session: () => storekeeperSession,
      stock_locations: () => locationsFixture,
      catalog_categories_list: () => categoriesFixture,
      catalog_units_list: () => unitsFixture,
      stock_levels_view: () => levelsFixture,
      stock_minimums_get: () => [{ locationId: SHOP, minQty: 6_000 }],
      stock_minimums_set: (args) => {
        saved.push(args);
        return [];
      },
    });
    renderWithProviders(<LevelsTab />);
    fireEvent.click(await screen.findByRole("button", { name: "Minimums — Lait UHT 1 L" }));
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByLabelText("Minimum — Magasin")).toHaveValue("6");
    // the damaged goods have no minimum
    expect(within(dialog).queryByLabelText("Minimum — Marchandise abîmée")).not.toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText("Minimum — Dépôt"), { target: { value: "24" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Enregistrer les minimums" }));
    await waitFor(() =>
      expect(saved).toEqual([
        {
          productId: "p-lait",
          minimums: [
            { locationId: SHOP, minQty: 6_000 },
            { locationId: "loc-depot", minQty: 24_000 },
          ],
        },
      ]),
    );
  });
});

describe("movement log", () => {
  /** Newest first: movement 0 is the latest, a transfer out with its BT number. */
  function movement(i: number): MovementRow {
    return {
      id: `m-${String(1_000 - i).padStart(4, "0")}`,
      createdAt: 1_790_600_000_000 - i * 60_000,
      productId: "p-lait",
      nameAr: "حليب 1 ل",
      nameFr: "Lait UHT 1 L",
      nameEn: "UHT milk 1 L",
      locationId: SHOP,
      qtyDelta: i === 0 ? -12_000 : -1_000,
      reason: i === 0 ? "transfer_out" : "sale",
      reasonCode: null,
      unitCost: null,
      refType: "document",
      refId: `d-${i}`,
      refNumber: i === 0 ? "BT2026-000001" : `TK2026-C1-${String(i).padStart(6, "0")}`,
      lotNumber: null,
      userId: null,
      note: null,
    };
  }

  it("pages through the ledger by the last row of each page", async () => {
    const queries: Record<string, unknown>[] = [];
    const first = Array.from({ length: 200 }, (_, i) => movement(i));
    mockCommands({
      auth_session: () => storekeeperSession,
      stock_locations: () => locationsFixture,
      stock_movement_log: (args) => {
        const query = args.query as Record<string, unknown>;
        queries.push(query);
        return query.before === null ? first : [movement(200)];
      },
    });
    renderWithProviders(<MovementsTab />);
    const transfer = await screen.findByRole("row", { name: /BT2026-000001/ });
    expect(within(transfer).getByText("Transfert envoyé")).toBeInTheDocument();
    expect(within(transfer).getByText("-12")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Plus récents/ })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: /Plus anciens/ }));
    const last = first[199];
    await waitFor(() =>
      expect(queries.at(-1)).toMatchObject({ before: last?.createdAt, beforeId: last?.id, limit: 200 }),
    );
    expect(await screen.findByText("Page 2")).toBeInTheDocument();
    expect(await screen.findByRole("row", { name: /TK2026-C1-000200/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Plus anciens/ })).toBeDisabled();
  });
});

describe("movement log search", () => {
  it("searches the whole ledger in the backend, not the page shown", async () => {
    const queries: Record<string, unknown>[] = [];
    mockCommands({
      auth_session: () => storekeeperSession,
      stock_locations: () => locationsFixture,
      stock_movement_log: (args) => {
        queries.push(args.query as Record<string, unknown>);
        return [];
      },
    });
    renderWithProviders(<MovementsTab />);
    fireEvent.change(await screen.findByLabelText("Rechercher"), { target: { value: "BT2026" } });
    await waitFor(() => expect(queries.at(-1)).toMatchObject({ text: "BT2026", before: null }));
  });
});

describe("serial numbers", () => {
  it("finds a serial number and sends its piece back to the supplier of its receipt", async () => {
    const piece: Serial = {
      id: "sn-1",
      productId: "p-milk",
      serialNo: "356938035643809",
      status: "in_stock",
      locationId: SHOP,
      receivedDocumentId: "d-rc1",
      saleLineId: null,
      updatedAt: 1_790_600_000_000,
    };
    const supplierHit: SupplierHit = {
      id: supplierFixture.id,
      code: supplierFixture.code,
      name: supplierFixture.name,
      phone: supplierFixture.phone,
      wilaya: supplierFixture.wilaya,
      balance: supplierFixture.balance,
      archived: false,
    };
    const queries: Record<string, unknown>[] = [];
    mockCommands({
      auth_session: () => storekeeperSession,
      stock_locations: () => locationsFixture,
      serials_list: (args) => {
        queries.push(args.query as Record<string, unknown>);
        return [
          piece,
          { ...piece, id: "sn-2", serialNo: "356938035643817", status: "sold", saleLineId: "l-9" },
        ];
      },
      products_get: () => productFixture,
      purchases_get: () => ({
        ...documentFixture,
        id: "d-rc1",
        docType: "goods_receipt",
        supplierId: "s-cevital",
      }),
      suppliers_search: () => [supplierHit],
      suppliers_get: () => supplierFixture,
      products_extras_get: () => ({ ...lotTrackedExtrasFixture, trackLots: false, trackSerials: true }),
      catalog_units_list: () => unitsFixture,
    });
    renderWithProviders(<SerialsTab />);
    fireEvent.change(await screen.findByLabelText("Rechercher"), { target: { value: "3569380356438" } });
    await waitFor(() => expect(queries.at(-1)).toMatchObject({ text: "3569380356438" }));
    const sold = await screen.findByRole("row", { name: /356938035643817/ });
    expect(within(sold).getByText("Liée à un ticket")).toBeInTheDocument();
    expect(within(sold).getByText("Vendu")).toBeInTheDocument();
    const inStock = screen.getByRole("row", { name: /356938035643809/ });
    expect(await within(inStock).findByText("Lait Candia 1 L")).toBeInTheDocument();
    fireEvent.click(
      await within(inStock).findByRole("button", { name: "Renvoyer au fournisseur — 356938035643809" }),
    );
    expect(within(sold).queryByRole("button")).not.toBeInTheDocument();
    const chooser = await screen.findByRole("dialog", { name: "Choisir le fournisseur" });
    await waitFor(() => expect(within(chooser).getByLabelText("Fournisseur")).toHaveValue("s-cevital"));
    fireEvent.click(within(chooser).getByRole("button", { name: "Suivant" }));
    const back = await screen.findByRole("dialog", { name: "Retour à Cevital" });
    expect(await within(back).findByText("356938035643809")).toBeInTheDocument();
    expect(within(back).getByLabelText("Quantité rendue — Lait Candia 1 L")).toHaveValue("1");
  });
});

describe("stock page", () => {
  it("opens on six tabs for the storekeeper and on the counts alone for a counter", async () => {
    mockCommands({
      auth_session: () => storekeeperSession,
      stock_locations: () => locationsFixture,
      catalog_categories_list: () => categoriesFixture,
      stock_levels_view: () => levelsFixture,
    });
    const { unmount } = renderWithProviders(<StockScreen />, { route: "/stock" });
    await waitFor(() => expect(screen.getAllByRole("tab")).toHaveLength(6));
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
      "État du stock",
      "Mouvements",
      "Inventaires",
      "Transferts",
      "Péremptions",
      "Numéros de série",
    ]);
    expect(await screen.findByRole("row", { name: /Lait UHT 1 L/ })).toBeInTheDocument();
    unmount();

    mockCommands({
      auth_session: () => counterSession,
      auth_list_users: () => [],
      counts_list: () => [countFixture],
    });
    renderWithProviders(<StockScreen />, { route: "/stock?tab=levels" });
    const counts = await screen.findByRole("tab", { name: "Inventaires" });
    expect(counts).toHaveAttribute("aria-selected", "true");
    expect(screen.getAllByRole("tab")).toHaveLength(1);
    expect(await screen.findByRole("row", { name: /En cours/ })).toBeInTheDocument();
  });
});
