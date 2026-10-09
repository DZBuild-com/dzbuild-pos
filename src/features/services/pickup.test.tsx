import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCart } from "@/features/pos/cart-store";
import { SaleScreen } from "@/features/pos/SaleScreen";
import { cashierSession, companyFixture, settingsValues } from "@/test/fixtures";
import {
  cashSessionFixture,
  categoriesFixture,
  previewFixture,
  priceLevelsFixture,
  productHitsFixture,
  saleDoneFixture,
} from "@/test/fixtures-m1";
import { keyMap, packState, paymentTypes, serviceTicket } from "@/test/fixtures-m8";
import { TillKitContext } from "@/features/pos/till-kit";
import { tillKit } from "@/features/pos/till-kit-parts";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";

vi.mock("@/lib/sounds", () => ({ beep: () => undefined }));

const hardware = {
  version: 1,
  printers: [],
  receiptPrinterId: null,
  receipt: {
    printByDefault: false,
    openDrawerOnCash: true,
    drawerBeforePrint: false,
    logo: true,
    secondLang: null,
  },
  scanner: { layout: "us" },
};

/** The backend's preview of the ticket's screen and labour: 18 000,00 DA, no duty; what is
 * missing comes from the tenders. */
function preview({ input }: Record<string, unknown>) {
  const { tenders } = input as { tenders: { amount: number }[] };
  const base = previewFixture(1_800_000, 0, 0);
  const paid = tenders.reduce((sum, t) => sum + t.amount, 0);
  return {
    ...base,
    settlement: { ...base.settlement, paidTotal: paid, missing: Math.max(0, 1_800_000 - paid), change: 0 },
  };
}

function mockTill(extra: Record<string, Handler>) {
  return mockCommands({
    auth_session: () => cashierSession,
    company_get: () => ({ ...companyFixture, defaultDocLang: "fr" }),
    settings_get: ({ key }) => settingsValues[String(key)] ?? null,
    hardware_get: () => hardware,
    cash_current: () => cashSessionFixture,
    catalog_categories_list: () => categoriesFixture,
    catalog_price_levels_list: () => priceLevelsFixture,
    products_grid: () => productHitsFixture,
    sales_held: () => [],
    sales_preview: preview,
    service_get: () => serviceTicket({ status: "ready" }),
    ...extra,
  });
}

function handBack() {
  return renderWithProviders(<SaleScreen session={cashierSession} />, {
    route: "/pos",
    state: { pickup: "st4" },
  });
}

describe("a repair handed back at the till", () => {
  beforeEach(() => {
    useCart.getState().clear("fr");
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(700);
  });

  it("loads the ticket's lines and customer, starts from the deposit and pays through service_pickup", async () => {
    const sent: unknown[] = [];
    mockTill({
      service_pickup: (args) => {
        sent.push(args);
        return {
          picked: { ticket: serviceTicket({ status: "delivered" }), sale: saleDoneFixture.sale },
          printQueued: false,
        };
      },
    });
    const user = userEvent.setup();
    handBack();
    const pay = await screen.findByRole("dialog", { name: "Encaissement" });
    expect(useCart.getState()).toMatchObject({ customerId: "c-nassim", note: "RP2026-000004", lang: "fr" });
    expect(useCart.getState().lines.map((l) => [l.line.productId, l.line.unitPrice])).toEqual([
      ["p-screen-a54", null],
      ["p-labour", 300_000],
    ]);
    // the 5 000,00 DA held, as the voucher tender naming the ticket
    expect(within(pay).getByLabelText("Avoir client")).toHaveValue("5000,00");
    await user.type(within(pay).getByLabelText("Espèces"), "13000");
    const validate = within(pay).getByRole("button", { name: "Valider" });
    await waitFor(() => expect(validate).toBeEnabled());
    await user.click(validate);
    // the backend applies the deposit itself: only the cash is sent
    await waitFor(() =>
      expect(sent[0]).toEqual({
        input: {
          id: "st4",
          payments: [
            { method: "cash", amount: 1_300_000, reference: null, chequeBank: null, chequeDate: null },
          ],
          extras: { serials: [], prescriptions: [], paymentTypes: [] },
        },
        printReceipt: false,
      }),
    );
    expect(await screen.findByRole("dialog", { name: "Vente encaissée" })).toBeInTheDocument();
    expect(useCart.getState().lines).toEqual([]);
  });

  it("sends a custom payment type with its own tender once the deposit is left out (final review)", async () => {
    const sent: unknown[] = [];
    mockTill({
      packs_state: () => packState({ modules: ["repairs"] }),
      till_key_map: () => keyMap(),
      payment_types_list: () => paymentTypes(),
      service_pickup: (args) => {
        sent.push(args);
        return {
          picked: { ticket: serviceTicket({ status: "delivered" }), sale: saleDoneFixture.sale },
          printQueued: false,
        };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(
      <TillKitContext.Provider value={tillKit}>
        <SaleScreen session={cashierSession} />
      </TillKitContext.Provider>,
      { route: "/pos", state: { pickup: "st4" } },
    );
    const pay = await screen.findByRole("dialog", { name: "Encaissement" });
    await user.click(await within(pay).findByRole("button", { name: "Prélèvement CCP" }));
    await user.type(within(pay).getByLabelText("Référence"), "PRLV 4471");
    await user.type(within(pay).getByLabelText("Prélèvement CCP"), "13000");
    const validate = within(pay).getByRole("button", { name: "Valider" });
    await waitFor(() => expect(validate).toBeEnabled());
    await user.click(validate);
    await waitFor(() => expect(sent).toHaveLength(1));
    // the deposit (payment 0 on screen) is applied by the backend: the type names the debit, now 0
    expect(sent[0]).toMatchObject({
      input: {
        payments: [{ method: "transfer", amount: 1_300_000, reference: "PRLV 4471" }],
        extras: { paymentTypes: [{ payment: 0, paymentTypeId: "00000000-0000-7000-8000-000000000601" }] },
      },
    });
  });

  it("never mixes a hand-back into a ticket being rung up", async () => {
    const calls = mockTill({});
    useCart.getState().addProduct({
      productId: "p-milk",
      qty: 1_000,
      display: { name: "Lait Candia 1 L", unitCode: "pc", unitDecimals: 0, price: 15_000 },
    });
    handBack();
    await waitFor(() => expect(calls).toContain("service_get"));
    expect(
      await screen.findByText("Mettez d'abord le ticket en cours en attente ou encaissez-le."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Encaissement" })).toBeNull();
    expect(useCart.getState().lines.map((l) => l.line.productId)).toEqual(["p-milk"]);
  });
});
