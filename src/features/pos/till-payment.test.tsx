import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "@/app/App";
import { useCart } from "@/features/pos/cart-store";
import { cashierSession, companyFixture, ownerTile, settingsValues } from "@/test/fixtures";
import {
  cashSessionFixture,
  categoriesFixture,
  previewFixture,
  priceLevelsFixture,
  productHitsFixture,
  saleDoneFixture,
  scanHitFixture,
  taxRatesFixture,
} from "@/test/fixtures-m1";
import { activeStatus } from "@/test/fixtures-m9";
import { type Handler, mockCommands } from "@/test/render";
import { scanUntilLooked } from "@/test/scan";
import { allowedOf, fixedOf, tillPayment } from "./till-payment";

// The whole app renders here (see payment.test.tsx): generous waits.
vi.setConfig({ testTimeout: 30_000 });
const WAIT = { timeout: 10_000 };

/** The codes the screen asked the backend for (see `scanUntilLooked`). */
let lookups = 0;

const printer = {
  version: 1,
  printers: [
    { id: "p1", name: "Caisse 1", transport: { kind: "windows", printerName: "XP-80C" }, profile: {} },
  ],
  receiptPrinterId: "p1",
  receipt: {
    printByDefault: true,
    openDrawerOnCash: true,
    drawerBeforePrint: false,
    logo: true,
    secondLang: null,
  },
  scanner: { layout: "us" },
};

/** The shop's BNA terminal: a card payment type without a reference. */
const bnaTerminal = {
  id: "pt-bna",
  code: "tpe_bna",
  nameAr: "جهاز BNA",
  nameFr: "TPE BNA",
  nameEn: "BNA terminal",
  baseMethod: "card_cib" as const,
  requiresReference: false,
  sortOrder: 0,
  archived: false,
};

type Tender = { method: string; amount: number };

/** The backend: 300,00 DA of milk; 5 DA duty when cash is used; change and missing from the tenders. */
function preview({ input }: Record<string, unknown>) {
  const tenders = (input as { tenders: Tender[] }).tenders ?? [];
  const cash = tenders.filter((p) => p.method === "cash").reduce((s, p) => s + p.amount, 0);
  const card = tenders.filter((p) => p.method !== "cash").reduce((s, p) => s + p.amount, 0);
  const duty = card >= 30_000 ? 0 : 500;
  const base = previewFixture(30_000, cash, duty);
  const paid = cash + card;
  const due = 30_000 + duty;
  return {
    ...base,
    settlement: {
      ...base.settlement,
      nonCash: card,
      cashTendered: cash,
      paidTotal: paid,
      missing: Math.max(0, due - paid),
      change: Math.max(0, paid - due),
      stampCharged: duty,
      stampDuty: duty,
      totalDue: due,
      tenders,
    },
  };
}

function mockTill(settings: Record<string, unknown>, extra: Record<string, Handler> = {}) {
  const values: Record<string, unknown> = { ...settingsValues, ...settings };
  return mockCommands({
    app_startup_error: () => null,
    app_startup_notices: () => [],
    license_status: () => activeStatus,
    setup_status: () => ({ completed: true, hasOwner: true, terminalCode: "C1", loginLang: "fr" }),
    auth_session: () => cashierSession,
    auth_list_users: () => [ownerTile],
    company_get: () => ({ ...companyFixture, defaultDocLang: "fr" }),
    settings_get: ({ key }) => values[String(key)] ?? null,
    datapack_get: () => ({ nifLengths: [15, 18], wilayas: [] }),
    hardware_get: () => printer,
    cash_current: () => cashSessionFixture,
    catalog_categories_list: () => categoriesFixture,
    catalog_price_levels_list: () => priceLevelsFixture,
    catalog_tax_rates_list: () => taxRatesFixture,
    products_grid: () => productHitsFixture,
    products_find_by_code: () => {
      lookups += 1;
      return { ...scanHitFixture, packQty: 2_000 };
    },
    payment_types_list: () => [bnaTerminal],
    sales_held: () => [],
    sales_preview: preview,
    ...extra,
  });
}

/** The cashier lands on the sale screen and scans two bottles of milk. */
async function ringUp() {
  const user = userEvent.setup();
  render(<App />);
  const field = await screen.findByLabelText("Scanner ou rechercher un produit", {}, WAIT);
  await scanUntilLooked(field, "6130000000017", () => lookups);
  await screen.findByText("2 × Lait Candia 1 L", {}, WAIT);
  return { user, field };
}

const RETURN = {
  originalId: "d-0",
  lines: [
    {
      originLineId: "l-0",
      productId: null,
      qty: 1_000,
      unitPrice: null,
      restock: true,
      reason: "changed_mind" as const,
      note: null,
    },
  ],
  refundMethod: "cash" as const,
  refundReference: null,
  lang: "fr" as const,
  note: null,
};

describe("the till's payment settings (pure)", () => {
  it("reads the defaults, a narrowed list and a vanished custom type", () => {
    expect(tillPayment(undefined, undefined, undefined, undefined)).toMatchObject({
      fixed: null,
      prefill: false,
      allowed: ["cash", "card_cib", "edahabia", "baridimob", "cheque", "transfer", "credit", "voucher"],
    });
    // the dialog's order, whatever the stored order; an empty or unreadable list is every method
    expect(allowedOf(["voucher", "card_cib"])).toEqual(["card_cib", "voucher"]);
    expect(allowedOf([])).toHaveLength(8);
    expect(tillPayment("ask", "cash", ["edahabia"], undefined)).toMatchObject({
      fixed: null,
      prefill: true,
      allowed: ["edahabia"],
    });
    expect(fixedOf("baridimob", undefined)).toEqual({ method: "baridimob", type: null });
    expect(fixedOf("type:pt-bna", [bnaTerminal])).toEqual({ method: "card_cib", type: bnaTerminal });
    // archived, asking a reference, or riding a cheque: the till asks instead
    expect(fixedOf("type:pt-bna", [{ ...bnaTerminal, archived: true }])).toBeNull();
    expect(fixedOf("type:pt-bna", [{ ...bnaTerminal, requiresReference: true }])).toBeNull();
    expect(fixedOf("type:pt-bna", [{ ...bnaTerminal, baseMethod: "cheque" }])).toBeNull();
    expect(fixedOf("type:pt-gone", [bnaTerminal])).toBeNull();
    expect(tillPayment("fixed", "type:pt-gone", undefined, [])).toMatchObject({
      fixed: null,
      prefill: false,
    });
  });
});

describe("Settings › Payments at the till", () => {
  beforeEach(() => {
    useCart.getState().clear("fr");
    lookups = 0;
  });

  it("one method, cash: F12 finishes the sale for the backend's amount due, without the dialog", async () => {
    const created: Record<string, unknown>[] = [];
    mockTill(
      { "payments.till_mode": "fixed", "payments.fixed_method": "cash" },
      {
        sales_create: (args) => {
          created.push(args);
          return saleDoneFixture;
        },
      },
    );
    const { user } = await ringUp();
    await user.keyboard("{F12}");
    const done = await screen.findByRole("dialog", { name: "Vente encaissée" }, WAIT);
    expect(screen.queryByRole("dialog", { name: "Encaissement" })).not.toBeInTheDocument();
    // 300,00 DA and the 5 DA stamp duty on a cash payment, as the backend counts them
    expect(created).toHaveLength(1);
    expect(created[0]).toMatchObject({
      request: {
        printReceipt: true,
        sale: { payments: [{ method: "cash", amount: 30_500, reference: null }] },
        extras: { paymentTypes: [] },
      },
    });
    expect((created[0]?.request as { sale: { payments: unknown[] } }).sale.payments).toHaveLength(1);
    expect(useCart.getState().lines).toHaveLength(0);
    expect(within(done).getByText("TK2026-C1-000123")).toBeInTheDocument();
    // Enter on the done screen starts the next sale
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Vente encaissée" })).toBeNull());
    expect(screen.getByLabelText("Scanner ou rechercher un produit")).toHaveFocus();
  });

  it("one method: Enter in the empty scan field pays like F12", async () => {
    const created: unknown[] = [];
    mockTill(
      { "payments.till_mode": "fixed", "payments.fixed_method": "cash" },
      {
        sales_create: (args) => {
          created.push(args);
          return saleDoneFixture;
        },
      },
    );
    const { user, field } = await ringUp();
    field.focus();
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("dialog", { name: "Vente encaissée" }, WAIT)).toBeInTheDocument();
    expect(created).toHaveLength(1);
  });

  it("one method, a custom card type: the card tender carries the type", async () => {
    const created: unknown[] = [];
    mockTill(
      { "payments.till_mode": "fixed", "payments.fixed_method": "type:pt-bna" },
      {
        sales_create: (args) => {
          created.push(args);
          return saleDoneFixture;
        },
      },
    );
    const { user } = await ringUp();
    await user.keyboard("{F12}");
    expect(await screen.findByRole("dialog", { name: "Vente encaissée" }, WAIT)).toBeInTheDocument();
    expect(created).toEqual([
      expect.objectContaining({
        request: expect.objectContaining({
          sale: expect.objectContaining({
            payments: [
              { method: "card_cib", amount: 30_000, reference: null, chequeBank: null, chequeDate: null },
            ],
          }),
          extras: expect.objectContaining({ paymentTypes: [{ payment: 0, paymentTypeId: "pt-bna" }] }),
        }),
      }),
    ]);
  });

  it("choose at the till: only the ticked methods, the first one filled in, Enter validates", async () => {
    const created: unknown[] = [];
    mockTill(
      { "payments.allowed_methods": ["card_cib", "edahabia"] },
      {
        sales_create: (args) => {
          created.push(args);
          return saleDoneFixture;
        },
      },
    );
    const { user } = await ringUp();
    await user.keyboard("{F12}");
    const dialog = await screen.findByRole("dialog", { name: "Encaissement" }, WAIT);
    expect(within(dialog).getByRole("button", { name: "Carte CIB" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Edahabia" })).toBeInTheDocument();
    for (const name of ["Espèces", "BaridiMob", "Chèque", "Virement", "Crédit client", "Avoir client"]) {
      expect(within(dialog).queryByRole("button", { name })).not.toBeInTheDocument();
    }
    // the payment type rides a card: it stays; no cash, so no note to add
    expect(await within(dialog).findByRole("button", { name: "TPE BNA" }, WAIT)).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: "200 DA" })).not.toBeInTheDocument();
    await waitFor(() => expect(within(dialog).getByLabelText("Carte CIB")).toHaveValue("300,00"), WAIT);
    await waitFor(() => expect(within(dialog).getByRole("button", { name: /Valider/ })).toBeEnabled(), WAIT);
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("dialog", { name: "Vente encaissée" }, WAIT)).toBeInTheDocument();
    expect(created).toEqual([
      expect.objectContaining({
        request: expect.objectContaining({
          sale: expect.objectContaining({
            payments: [expect.objectContaining({ method: "card_cib", amount: 30_000 })],
          }),
        }),
      }),
    ]);
  });

  it("one method with a pending exchange: the dialog opens on that method with the amount", async () => {
    const created: unknown[] = [];
    const exchanged: unknown[] = [];
    mockTill(
      { "payments.till_mode": "fixed", "payments.fixed_method": "cash" },
      {
        sales_create: (args) => {
          created.push(args);
          return saleDoneFixture;
        },
        returns_exchange: (args) => {
          exchanged.push(args);
          return Promise.reject({ code: "unknown", message: "not in this test" });
        },
      },
    );
    useCart.getState().startExchange({ input: RETURN, value: 10_000, originalNumber: "TK2026-C1-000100" });
    const { user } = await ringUp();
    await user.keyboard("{F12}");
    const dialog = await screen.findByRole("dialog", { name: "Encaissement" }, WAIT);
    await waitFor(() => expect(within(dialog).getByLabelText("Espèces")).toHaveValue("305,00"), WAIT);
    expect(created).toHaveLength(0);
    expect(exchanged).toHaveLength(0);
  });

  it("one method: a refusal opens the dialog on the tender tried, with the backend's answer", async () => {
    mockTill(
      { "payments.till_mode": "fixed", "payments.fixed_method": "cash" },
      { sales_create: () => Promise.reject({ code: "stock_insufficient", details: { shortages: [] } }) },
    );
    const { user } = await ringUp();
    await user.keyboard("{F12}");
    const dialog = await screen.findByRole("dialog", { name: "Encaissement" }, WAIT);
    expect(within(dialog).getByLabelText("Espèces")).toHaveValue("305,00");
    expect(within(dialog).getByRole("alert")).toBeInTheDocument();
  });
});
