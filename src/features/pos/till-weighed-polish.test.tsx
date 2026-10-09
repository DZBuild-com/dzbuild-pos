import { act, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useCart } from "@/features/pos/cart-store";
import { SaleScreen } from "@/features/pos/SaleScreen";
import { ScalePanel } from "@/features/pos/ScalePanel";
import { resetCustomerDisplayFeed } from "@/features/pos/use-customer-display";
import type { LabelPrintDone, WeighedLabelInput } from "@/lib/ipc/types";
import { setBurstGuard } from "@/lib/scanner";
import { cashierSession, companyFixture } from "@/test/fixtures";
import { cashSessionFixture, previewFixture } from "@/test/fixtures-m1";
import { hardwareV2Fixture, weightFixture } from "@/test/fixtures-m4";
import { type Handler, missingKeys, mockCommands, renderWithProviders } from "@/test/render";

const tomato = {
  id: "p-tomato",
  reference: null,
  nameAr: "طماطم",
  nameFr: "Tomates",
  nameEn: "Tomatoes",
  categoryId: "c-veg",
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
  barcode: null,
  stock: 10_000,
};

const scaleConfig = {
  serial: { port: "COM3", baud: 9600, dataBits: "eight", parity: "none", stopBits: "one" },
  protocol: "continuous",
  pollMs: 250,
  stableReadings: 3,
  capacityG: 30000,
};

const printed = (warnings: LabelPrintDone["warnings"] = []): LabelPrintDone => ({
  outcome: { printer: "Étiquettes", bytes: 2_048, raster: true },
  labels: 1,
  assigned: [],
  warnings,
});

function mockTill(extra: Record<string, Handler> = {}) {
  return mockCommands({
    auth_session: () => cashierSession,
    company_get: () => companyFixture,
    hardware_get: () => ({ ...hardwareV2Fixture, scale: scaleConfig }),
    cash_current: () => cashSessionFixture,
    sales_held: () => [],
    products_grid: ({ categoryId }) => (categoryId === "c-veg" ? [tomato] : []),
    catalog_categories_list: () => [],
    sales_preview: () => previewFixture(30_000, 0),
    scale_latest: () => weightFixture,
    scale_read_stable: () => weightFixture,
    display_show: () => null,
    ...extra,
  });
}

function renderPanel() {
  renderWithProviders(
    <ScalePanel
      categoryId="c-veg"
      onAdd={() => undefined}
      onWeighing={() => undefined}
      onClose={() => undefined}
    />,
  );
}

/** Picks the tomatoes on the panel and asks for their label. */
async function printTomatoLabel() {
  await userEvent.click(await screen.findByRole("button", { name: /Tomates/ }));
  const print = screen.getByRole("button", { name: "Imprimer l'étiquette" });
  await waitFor(() => expect(print).toBeEnabled());
  await userEvent.click(print);
}

beforeEach(() => {
  setBurstGuard(false);
  resetCustomerDisplayFeed();
  useCart.getState().clear("fr");
  missingKeys.clear();
});

describe("weighing at the till", () => {
  it("the cart's 'Peser (F11)' button opens the scale panel", async () => {
    mockTill();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    const weigh = await screen.findByRole("button", { name: "Peser" });
    expect(weigh).toHaveAttribute("aria-keyshortcuts", "F11");
    await userEvent.click(weigh);
    expect(await screen.findByLabelText("Article à peser")).toBeInTheDocument();
    expect(screen.getByTestId("scale-weight")).toHaveTextContent("1,25");
  });

  it("the cart offers no 'Peser' button on a PC without a scale", async () => {
    mockTill({ hardware_get: () => ({ ...hardwareV2Fixture, scale: null }) });
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await screen.findByRole("combobox", { name: /scanner/i });
    await waitFor(() => expect(screen.getByRole("button", { name: /Encaisser/ })).toBeInTheDocument());
    expect(screen.queryByRole("button", { name: "Peser" })).not.toBeInTheDocument();
  });

  it("a food label without Arabic prints from the panel only once the cashier confirms", async () => {
    const sent: WeighedLabelInput[] = [];
    mockTill({
      labels_print_weighed: ({ input }) => {
        sent.push(input as WeighedLabelInput);
        return (input as WeighedLabelInput).arabicOverride
          ? printed()
          : Promise.reject({ code: "label_arabic_required", details: { productIds: ["p-tomato"] } });
      },
    });
    renderPanel();
    await printTomatoLabel();
    const guard = await screen.findByRole("alert");
    expect(guard).toHaveTextContent(
      "Cet article alimentaire ou réglementé doit avoir une étiquette en arabe (décret 13-378).",
    );
    expect(sent).toEqual([expect.objectContaining({ arabicOverride: false })]);
    await userEvent.click(screen.getByRole("button", { name: "Imprimer sans l'arabe" }));
    await userEvent.click(screen.getByRole("button", { name: "Oui, imprimer en français seulement" }));
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1]).toEqual({ ...sent[0], arabicOverride: true });
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    expect([...missingKeys]).toEqual([]);
  });

  it("the panel switches a label without Arabic to Arabic + French, at the weight on the scale now", async () => {
    const sent: WeighedLabelInput[] = [];
    let onScale = weightFixture;
    mockTill({
      scale_read_stable: () => onScale,
      labels_print_weighed: ({ input }) => {
        const asked = input as WeighedLabelInput;
        sent.push(asked);
        return asked.request.lang === "ar_fr"
          ? printed()
          : Promise.reject({ code: "label_arabic_required", details: { productIds: ["p-tomato"] } });
      },
    });
    renderPanel();
    await printTomatoLabel();
    const switchLang = await screen.findByRole("button", { name: "Passer en arabe + français" });
    expect(sent[0]?.request.net).toBe(1_250);
    // the tomatoes are weighed again before the cashier answers the guard
    onScale = { ...weightFixture, gross: 1_480, net: 1_480 };
    await act(() => emit("weight-changed", { reading: onScale }));
    await userEvent.click(switchLang);
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1]).toEqual({
      ...sent[0],
      request: { ...sent[0]!.request, lang: "ar_fr", net: 1_480 },
      arabicOverride: false,
    });
  });

  it("waits for a stable weight before the guard's buttons print again", async () => {
    mockTill({
      labels_print_weighed: () =>
        Promise.reject({ code: "label_arabic_required", details: { productIds: ["p-tomato"] } }),
    });
    renderPanel();
    await printTomatoLabel();
    const switchLang = await screen.findByRole("button", { name: "Passer en arabe + français" });
    await act(() =>
      emit("weight-changed", { reading: { ...weightFixture, net: 900, stable: false, state: "motion" } }),
    );
    await waitFor(() => expect(switchLang).toBeDisabled());
  });

  it("prices the kilo as the till charges it: TTC, with the customer's level and today's promotion", async () => {
    const asked: unknown[] = [];
    const weighing: Array<number | null> = [];
    mockTill({
      sales_preview: ({ input }) => {
        asked.push(input);
        const preview = previewFixture(14_280, 0);
        return {
          ...preview,
          cart: {
            ...preview.cart,
            lines: [{ ...preview.cart.lines[0]!, unitPrice: 12_000, unitPriceTtc: 14_280 }],
          },
        };
      },
    });
    useCart.getState().setCustomer("c-amine");
    renderWithProviders(
      <ScalePanel
        categoryId="c-veg"
        onAdd={() => undefined}
        onWeighing={(w) => weighing.push(w?.unitPrice ?? null)}
        onClose={() => undefined}
      />,
    );
    await userEvent.click(await screen.findByRole("button", { name: /Tomates/ }));
    expect(await screen.findByTestId("kilo-price")).toHaveTextContent("142,80");
    expect(asked.at(-1)).toMatchObject({
      cart: { customerId: "c-amine", lines: [{ productId: "p-tomato", qty: 1_000 }] },
    });
    await waitFor(() => expect(weighing.at(-1)).toBe(14_280));
  });

  it("a product without its Arabic name is confirmed, never switched", async () => {
    mockTill({
      labels_print_weighed: () =>
        Promise.reject({ code: "label_arabic_name_missing", details: { productIds: ["p-tomato"] } }),
    });
    renderPanel();
    await printTomatoLabel();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Cet article alimentaire ou réglementé n'a pas de nom en arabe",
    );
    expect(screen.queryByRole("button", { name: "Passer en arabe + français" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Imprimer sans l'arabe" })).toBeInTheDocument();
  });

  it("a weighed label too small for its expiry date says so", async () => {
    mockTill({
      labels_print_weighed: () =>
        printed([{ productId: "p-tomato", name: "طماطم", code: "lot_or_expiry_dropped" }]),
    });
    renderPanel();
    await printTomatoLabel();
    expect(
      await screen.findByText(
        "L'étiquette est trop petite pour le lot ou la date limite, qui ne sont pas imprimés. Choisissez un modèle plus grand.",
      ),
    ).toBeInTheDocument();
  });
});
