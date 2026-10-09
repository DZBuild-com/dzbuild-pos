import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { emit } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it } from "vitest";
import { useCart } from "@/features/pos/cart-store";
import { SaleDoneDialog } from "@/features/pos/SaleDoneDialog";
import { SaleScreen } from "@/features/pos/SaleScreen";
import { ScalePanel } from "@/features/pos/ScalePanel";
import {
  resetCustomerDisplayFeed,
  useCustomerDisplayFeed,
  usePaidFrame,
} from "@/features/pos/use-customer-display";
import { setBurstGuard } from "@/lib/scanner";
import { cashierSession, companyFixture } from "@/test/fixtures";
import { cashSessionFixture, previewFixture, saleDoneFixture } from "@/test/fixtures-m1";
import { hardwareV2Fixture, weightFixture } from "@/test/fixtures-m4";
import { type Handler, mockCommands, renderWithProviders, scan } from "@/test/render";

const tomato = {
  id: "p-tomato",
  reference: null,
  nameAr: "طماطم",
  nameFr: "Tomates",
  nameEn: "Tomatoes",
  categoryId: null,
  unitId: "u-kg",
  unitCode: "kg",
  unitDecimals: 3,
  taxRateId: "t9",
  taxRateBp: 900,
  kind: "simple",
  parentId: null,
  sellPrice: 12_000,
  costPrice: 0,
  minStock: 0,
  trackStock: true,
  allowNegativeStock: false,
  isWeighed: true,
  availableAtTill: true,
  plu: 10012,
  image: null,
  archived: false,
  barcodes: [],
  prices: [],
  components: [],
  updatedAt: 0,
};

const scaleHit = (grams: number, difference = 0) => ({
  product: tomato,
  packQty: grams,
  scale: {
    code: { prefix: "21", plu: 10012, value: "weight", raw: grams },
    labelPrice: null,
    till: null,
    difference,
  },
});

const preview = previewFixture(30_000, 0);

const scaleConfig = {
  serial: { port: "COM3", baud: 9600, dataBits: "eight", parity: "none", stopBits: "one" },
  protocol: "continuous",
  pollMs: 250,
  stableReadings: 3,
  capacityG: 30000,
};

const ignoreWeighing = () => undefined;

function mockTill(extra: Record<string, Handler> = {}) {
  const shown: unknown[] = [];
  const calls = mockCommands({
    auth_session: () => cashierSession,
    company_get: () => companyFixture,
    hardware_get: () => ({ ...hardwareV2Fixture, scale: scaleConfig }),
    cash_current: () => cashSessionFixture,
    sales_held: () => [],
    products_grid: () => [],
    catalog_categories_list: () => [],
    sales_preview: () => preview,
    scale_latest: () => weightFixture,
    display_show: (args) => {
      shown.push(args.frame);
      return null;
    },
    ...extra,
  });
  return { shown, calls };
}

beforeEach(() => {
  setBurstGuard(false);
  resetCustomerDisplayFeed();
  useCart.getState().clear("fr");
});

describe("till hardware", () => {
  it("a scanned weight label adds its own line with the label's weight", async () => {
    const codes = ["2110012017502", "2110012012507"];
    mockTill({ products_find_by_code: (args) => scaleHit(args.code === codes[0] ? 1_750 : 1_250) });
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await screen.findByRole("combobox", { name: /scanner/i });
    scan(codes[0]!);
    scan(codes[1]!);
    await waitFor(() => expect(useCart.getState().lines.map((l) => l.line.qty)).toEqual([1_750, 1_250]));
  });

  it("a price label that differs from the till's amount says by how much", async () => {
    mockTill({ products_find_by_code: () => scaleHit(1_750, 3) });
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await screen.findByRole("combobox", { name: /scanner/i });
    scan("2110012017502");
    expect(await screen.findByText(/^Étiquette de balance : 0,03/)).toBeInTheDocument();
    expect(useCart.getState().lines.map((l) => l.line.qty)).toEqual([1_750]);
  });

  it("F11 adds nothing until the weight is stable", async () => {
    let stable = false;
    mockTill({
      products_search: () => [{ ...tomato, sellPrice: 12_000, barcode: null, stock: 10_000 }],
      scale_read_stable: () => (stable ? weightFixture : Promise.reject({ code: "scale_unstable" })),
    });
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await userEvent.keyboard("{F11}");
    await userEvent.type(await screen.findByLabelText("Article à peser"), "Tom");
    await userEvent.click(await screen.findByRole("button", { name: /Tomates/ }));
    await userEvent.click(screen.getByRole("button", { name: /Ajouter/ }));
    expect(
      await screen.findByText(
        "Le poids n'est pas stable. Attendez que l'article ne bouge plus, puis pesez de nouveau.",
      ),
    ).toBeInTheDocument();
    expect(useCart.getState().lines).toHaveLength(0);
    stable = true;
    await userEvent.click(screen.getByRole("button", { name: /Ajouter/ }));
    await waitFor(() => expect(useCart.getState().lines[0]?.line.qty).toBe(1_250));
    expect(useCart.getState().lines[0]?.display.latinName).toBe("Tomates");
  });

  it.each([
    ["disconnected", { ...weightFixture, stable: false, state: "disconnected" }, scaleConfig, "Débranchée"],
    ["unset", null, null, "Aucune balance"],
  ])("a %s scale shows its state and cannot add", async (_, reading, scale, label) => {
    mockTill({
      hardware_get: () => ({ ...hardwareV2Fixture, scale }),
      scale_latest: () => reading,
      // the weighed products of the category chosen on the sale screen, before anything is typed
      products_grid: ({ categoryId }) =>
        categoryId === "c-veg" ? [{ ...tomato, barcode: null, stock: 10_000 }] : [],
    });
    renderWithProviders(
      <ScalePanel
        categoryId="c-veg"
        onAdd={() => undefined}
        onWeighing={ignoreWeighing}
        onClose={() => undefined}
      />,
    );
    expect(await screen.findByText(label)).toBeInTheDocument();
    await userEvent.click(await screen.findByRole("button", { name: /Tomates/ }));
    expect(screen.getByRole("button", { name: /Ajouter/ })).toBeDisabled();
  });

  it("the live weight follows weight-changed in the panel", async () => {
    mockTill();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await userEvent.keyboard("{F11}");
    expect(await screen.findByTestId("scale-weight")).toHaveTextContent("1,25");
    await act(() =>
      emit("weight-changed", {
        reading: { ...weightFixture, gross: 2_340, net: 2_340, stable: false, state: "motion" },
      }),
    );
    expect(await screen.findByTestId("scale-weight")).toHaveTextContent("2,34");
    expect(screen.getByText("En mouvement")).toBeInTheDocument();
  });

  it("the customer display gets the backend's line and total, and idle when the cart empties", async () => {
    const { shown } = mockTill({
      products_find_by_code: () => ({ product: tomato, packQty: 1_000, scale: null }),
    });
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await screen.findByRole("combobox", { name: /scanner/i });
    scan("6130123456782");
    await waitFor(() =>
      expect(shown.at(-1)).toMatchObject({
        mode: "sale",
        total: preview.settlement.totalDue,
        items: preview.cart.itemCount,
        line: {
          name: "Lait Candia 1 L",
          latinName: "Tomates",
          total: preview.cart.lines[0]!.amounts.totalTtc,
        },
      }),
    );
    act(() => useCart.getState().clear("fr"));
    await waitFor(() => expect(shown.at(-1)).toMatchObject({ mode: "idle" }));
  });

  it("the customer display shows what one unit costs TTC, in an HT shop too", async () => {
    const ht = {
      ...preview,
      cart: {
        ...preview.cart,
        lines: [{ ...preview.cart.lines[0]!, unitPrice: 12_605, unitPriceTtc: 15_000 }],
      },
    };
    const { shown } = mockTill({
      sales_preview: () => ht,
      products_find_by_code: () => ({ product: tomato, packQty: 1_000, scale: null }),
    });
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await screen.findByRole("combobox", { name: /scanner/i });
    scan("6130123456782");
    await waitFor(() => expect(shown.at(-1)).toMatchObject({ mode: "sale", line: { unitPrice: 15_000 } }));
  });

  it("the customer display shows the weight while the panel is open", async () => {
    const { shown } = mockTill();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await userEvent.keyboard("{F11}");
    await waitFor(() =>
      expect(shown.at(-1)).toMatchObject({ mode: "sale", total: 0, weight: { net: 1_250, stable: true } }),
    );
  });

  it("no idle frame covers the thanks of a paid sale", async () => {
    const { shown } = mockTill();
    function PaidThenEmpty() {
      usePaidFrame({ lang: "fr", total: 30_000, paid: 40_000, change: 10_000 });
      useCustomerDisplayFeed(null);
      return null;
    }
    renderWithProviders(<PaidThenEmpty />);
    await waitFor(() => expect(shown).toHaveLength(1));
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(shown).toEqual([expect.objectContaining({ mode: "paid", change: 10_000 })]);
  });

  it("a validated sale sends the amount due, the amount paid and the change once", async () => {
    const { shown } = mockTill();
    const { settlement } = saleDoneFixture.sale;
    renderWithProviders(
      <SaleDoneDialog done={{ kind: "sale", result: saleDoneFixture }} onNewSale={() => undefined} />,
    );
    // the session answering renders the dialog again: still one frame
    await screen.findByRole("dialog");
    await waitFor(() =>
      expect(shown).toEqual([
        expect.objectContaining({
          mode: "paid",
          lang: "fr",
          total: settlement.totalDue,
          paid: settlement.paidTotal,
          change: settlement.change,
        }),
      ]),
    );
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(shown).toHaveLength(1);
  });

  it("a serial scanner's code goes through the same lookup", async () => {
    const { calls } = mockTill({
      products_find_by_code: () => ({ product: tomato, packQty: 1_000, scale: null }),
    });
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await screen.findByRole("combobox", { name: /scanner/i });
    await act(() => emit("serial-scanned", { code: "6130123456782" }));
    await waitFor(() => expect(calls).toContain("products_find_by_code"));
    await waitFor(() => expect(useCart.getState().lines).toHaveLength(1));
  });
});
