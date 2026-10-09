import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Toaster } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCart } from "@/features/pos/cart-store";
import { SaleScreen } from "@/features/pos/SaleScreen";
import type { Cart, Session } from "@/lib/ipc/types";
import { cashierSession, companyFixture, settingsValues } from "@/test/fixtures";
import {
  cashSessionFixture,
  categoriesFixture,
  previewFixture,
  priceLevelsFixture,
  productHitsFixture,
  saleDoneFixture,
} from "@/test/fixtures-m1";
import { order, packState } from "@/test/fixtures-m8";
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

/** Amina's sofa as the till's cart, at the ordered price. */
const sofaCart: Cart = {
  lines: [
    {
      productId: "p-sofa",
      description: null,
      qty: 1_000,
      unitPrice: 6_000_000,
      discount: { kind: "none" },
      taxRateId: null,
      note: null,
      expectedUnitPrice: 6_000_000,
    },
  ],
  priceLevelId: null,
  customerId: "c-amina",
  lang: "fr",
  note: null,
};

/** The backend's preview of the sofa: 60 000,00 DA, no duty; what is missing comes from the
 * tenders. */
function preview({ input }: Record<string, unknown>) {
  const { tenders } = input as { tenders: { amount: number }[] };
  const base = previewFixture(6_000_000, 0, 0);
  const paid = tenders.reduce((sum, t) => sum + t.amount, 0);
  return {
    ...base,
    settlement: { ...base.settlement, paidTotal: paid, missing: Math.max(0, 6_000_000 - paid), change: 0 },
  };
}

function mockTill(extra: Record<string, Handler>, session: Session = cashierSession) {
  return mockCommands({
    auth_session: () => session,
    company_get: () => ({ ...companyFixture, defaultDocLang: "fr" }),
    settings_get: ({ key }) =>
      key === "orders.layaway_min_deposit_bp" ? 2_000 : (settingsValues[String(key)] ?? null),
    hardware_get: () => hardware,
    cash_current: () => cashSessionFixture,
    catalog_categories_list: () => categoriesFixture,
    catalog_price_levels_list: () => priceLevelsFixture,
    products_grid: () => productHitsFixture,
    sales_held: () => [],
    sales_preview: preview,
    packs_state: () => packState({ modules: ["orders"] }),
    ...extra,
  });
}

describe("customer orders at the till", () => {
  beforeEach(() => {
    useCart.getState().clear("fr");
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(700);
  });

  it("serves an order: its lines and customer, the deposit as a CC voucher, paid through orders_fulfil", async () => {
    const sent: unknown[] = [];
    mockTill({
      orders_get: () => order({ held: 1_000_000 }),
      orders_fulfil_cart: () => sofaCart,
      orders_fulfil: (args) => {
        sent.push(args);
        return {
          fulfilled: { order: order({ status: "fulfilled" }), sale: saleDoneFixture.sale },
          printQueued: false,
        };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos", state: { fulfil: "o1" } });
    const pay = await screen.findByRole("dialog", { name: "Encaissement" });
    expect(useCart.getState()).toMatchObject({ customerId: "c-amina", note: "CC2026-000003" });
    expect(useCart.getState().lines.map((l) => [l.line.productId, l.line.unitPrice])).toEqual([
      ["p-sofa", 6_000_000],
    ]);
    // the 10 000,00 DA held, as the voucher tender naming the order (with a voucher module on, the
    // voucher tile is the "Bon d'achat" that reads deposits and vouchers, Task 24)
    expect(within(pay).getByLabelText("Bon d'achat")).toHaveValue("10000,00");
    await user.type(within(pay).getByLabelText("Espèces"), "50000");
    const validate = within(pay).getByRole("button", { name: "Valider" });
    await waitFor(() => expect(validate).toBeEnabled());
    await user.click(validate);
    // the voucher naming `CC…` spends the deposit (`orders::fulfil`), the cash pays the rest
    await waitFor(() =>
      expect(sent[0]).toEqual({
        input: {
          id: "o1",
          payments: [
            {
              method: "voucher",
              amount: 1_000_000,
              reference: "CC2026-000003",
              chequeBank: null,
              chequeDate: null,
            },
            { method: "cash", amount: 5_000_000, reference: null, chequeBank: null, chequeDate: null },
          ],
          extras: { serials: [], prescriptions: [], paymentTypes: [] },
        },
        printReceipt: false,
      }),
    );
    expect(await screen.findByRole("dialog", { name: "Vente encaissée" })).toBeInTheDocument();
    expect(useCart.getState().lines).toEqual([]);
  });

  it("puts the till's cart on order from the menu and empties the till", async () => {
    const created: unknown[] = [];
    const session = {
      ...cashierSession,
      permissions: [...cashierSession.permissions, "documents.quote" as const],
    };
    mockTill(
      {
        orders_create: (args) => {
          created.push(args);
          return order();
        },
      },
      session,
    );
    useCart
      .getState()
      .load(sofaCart, [{ name: "Canapé 3 places", unitCode: "pc", unitDecimals: 0, price: 6_000_000 }]);
    const user = userEvent.setup();
    renderWithProviders(
      <>
        <SaleScreen session={session} />
        <Toaster />
      </>,
      { route: "/pos" },
    );
    await screen.findByText("1 × Canapé 3 places");
    await user.keyboard("{Escape}");
    const menu = await screen.findByRole("dialog", { name: "Menu" });
    await user.click(await within(menu).findByRole("button", { name: "Mettre en commande" }));
    const dialog = await screen.findByRole("dialog", { name: "Mettre en commande" });
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer la commande" }));
    await waitFor(() =>
      expect(created[0]).toEqual({
        input: {
          draftId: null,
          cart: sofaCart,
          kind: "order",
          dueOn: null,
          reserve: false,
          deposit: null,
          note: null,
        },
      }),
    );
    await waitFor(() => expect(useCart.getState().lines).toEqual([]));
    expect(await screen.findByText("Commande CC2026-000003 enregistrée")).toBeInTheDocument();
  });
});
