import { emit } from "@tauri-apps/api/event";
import { cleanup, configure, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "@/app/App";
import { useCart } from "@/features/pos/cart-store";
import { setLanguage } from "@/lib/i18n";
import { cashierSession, companyFixture, ownerTile, settingsValues } from "@/test/fixtures";
import { activeStatus } from "@/test/fixtures-m9";
import {
  cashSessionFixture,
  categoriesFixture,
  documentFixture,
  previewFixture,
  priceLevelsFixture,
  productHitsFixture,
  saleDoneFixture,
  scanHitFixture,
  taxRatesFixture,
} from "@/test/fixtures-m1";
import { type Handler, mockCommands } from "@/test/render";
import { render } from "@testing-library/react";
import { scanUntilLooked } from "@/test/scan";

// The whole app renders here, and a parallel Rust build on the same PC can slow it down past the
// 3 s of the other suites: these waits get 10 s, and each test 30 s.
configure({ asyncUtilTimeout: 10_000 });
vi.setConfig({ testTimeout: 30_000 });

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

function mockSale(extra: Record<string, Handler> = {}) {
  return mockCommands({
    app_startup_error: () => null,
    app_startup_notices: () => [],
    // M9: the licence (a lifetime licence: the till sells)
    license_status: () => activeStatus,
    setup_status: () => ({ completed: true, hasOwner: true, terminalCode: "C1", loginLang: "fr" }),
    auth_session: () => cashierSession,
    auth_list_users: () => [ownerTile],
    company_get: () => ({ ...companyFixture, defaultDocLang: "fr" }),
    settings_get: ({ key }) => settingsValues[String(key)] ?? null,
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
    sales_held: () => [],
    sales_preview: preview,
    ...extra,
  });
}

async function openPayment() {
  const user = userEvent.setup();
  render(<App />);
  // a cashier lands on the sale screen
  const field = await screen.findByLabelText("Scanner ou rechercher un produit");
  await scanUntilLooked(field, "6130000000017", () => lookups);
  await screen.findByText("2 × Lait Candia 1 L");
  await user.keyboard("{F12}");
  const dialog = await screen.findByRole("dialog", { name: "Encaissement" });
  return { user, dialog };
}

describe("PaymentDialog", () => {
  beforeEach(() => useCart.getState().clear("fr"));

  it("shows the backend settlement and never computes money", async () => {
    const previews: unknown[] = [];
    mockSale({
      sales_preview: (args) => {
        previews.push(args);
        return preview(args);
      },
    });
    const { user, dialog } = await openPayment();
    expect(within(dialog).getByRole("status", { name: "Reste à payer" })).toHaveTextContent("305,00");
    expect(within(dialog).getByText("Droit de timbre")).toBeInTheDocument();
    expect(within(dialog).getByText(/Payer par carte évite/)).toBeInTheDocument();
    for (const key of ["5", "0", "0"]) await user.click(within(dialog).getByRole("button", { name: key }));
    await waitFor(() =>
      expect(within(dialog).getByRole("status", { name: "Rendu" })).toHaveTextContent("195,00"),
    );
    expect(previews.at(-1)).toMatchObject({
      input: {
        tenders: [{ method: "cash", amount: 50_000, reference: null, chequeBank: null, chequeDate: null }],
      },
    });
  });

  it("keeps two decimals when the keypad types after the comma", async () => {
    const previews: unknown[] = [];
    mockSale({
      sales_preview: (args) => {
        previews.push(args);
        return preview(args);
      },
    });
    const { user, dialog } = await openPayment();
    for (const key of ["5", ",", "5", "00"]) {
      await user.click(within(dialog).getByRole("button", { name: key }));
    }
    expect(within(dialog).getByLabelText("Espèces")).toHaveValue("5,50");
    await waitFor(() =>
      expect(previews.at(-1)).toMatchObject({ input: { tenders: [{ method: "cash", amount: 550 }] } }),
    );
  });

  it("adds quick notes and pays the exact amount", async () => {
    const previews: unknown[] = [];
    mockSale({
      sales_preview: (args) => {
        previews.push(args);
        return preview(args);
      },
    });
    const { user, dialog } = await openPayment();
    await user.click(within(dialog).getByRole("button", { name: "200 DA" }));
    await user.click(within(dialog).getByRole("button", { name: "100 DA" }));
    await waitFor(() =>
      expect(previews.at(-1)).toMatchObject({ input: { tenders: [{ method: "cash", amount: 30_000 }] } }),
    );
    await user.click(within(dialog).getByRole("button", { name: "Montant exact" }));
    await waitFor(() =>
      expect(previews.at(-1)).toMatchObject({ input: { tenders: [{ method: "cash", amount: 30_500 }] } }),
    );
    expect(within(dialog).getByRole("button", { name: /Valider/ })).toBeEnabled();
  });

  it("splits between cash and a CIB card with its authorisation number (PAY-002, PAY-004)", async () => {
    const previews: unknown[] = [];
    mockSale({
      sales_preview: (args) => {
        previews.push(args);
        return preview(args);
      },
    });
    const { user, dialog } = await openPayment();
    for (const key of ["1", "0", "0"]) await user.click(within(dialog).getByRole("button", { name: key }));
    await user.click(within(dialog).getByRole("button", { name: "Carte CIB" }));
    await user.type(within(dialog).getByLabelText("N° d'autorisation (Carte CIB)"), "A12345");
    await waitFor(() =>
      expect(previews.at(-1)).toMatchObject({
        input: {
          tenders: [
            { method: "cash", amount: 10_000 },
            { method: "card_cib", amount: 20_000, reference: "A12345" },
          ],
        },
      }),
    );
  });

  it("keeps credit and store credit for an identified customer (F3)", async () => {
    mockSale();
    const { dialog } = await openPayment();
    const hint = "Choisissez d'abord le client (F3) pour payer à crédit ou avec son avoir.";
    for (const name of ["Crédit client", "Avoir client"]) {
      const tile = within(dialog).getByRole("button", { name });
      expect(tile).toBeDisabled();
      expect(tile).toHaveAccessibleDescription(hint);
    }
    expect(within(dialog).getByRole("button", { name: "Carte CIB" })).toBeEnabled();
  });

  it("shows the change at once and offers a retry when the receipt fails", async () => {
    const created: unknown[] = [];
    const calls = mockSale({
      sales_create: (args) => {
        created.push(args);
        return saleDoneFixture;
      },
      hardware_retry_print: () => null,
    });
    const { user, dialog } = await openPayment();
    for (const key of ["5", "0", "0"]) await user.click(within(dialog).getByRole("button", { name: key }));
    await waitFor(() => expect(within(dialog).getByRole("button", { name: /Valider/ })).toBeEnabled());
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("dialog", { name: "Vente encaissée" })).toBeInTheDocument();
    // the committed ticket has left the cart already: a lock or a logout now cannot bring it back
    expect(useCart.getState().lines).toHaveLength(0);
    expect(screen.getByRole("status", { name: "Rendu" })).toHaveTextContent("195,00");
    expect(screen.getByText("TK2026-C1-000123")).toBeInTheDocument();
    expect(created[0]).toMatchObject({
      request: { printReceipt: true, sale: { payments: [{ method: "cash", amount: 50_000 }] } },
    });
    await user.click(screen.getByRole("button", { name: /Nouvelle vente/ }));
    expect(screen.queryByRole("dialog", { name: "Vente encaissée" })).not.toBeInTheDocument();
    expect(useCart.getState().lines).toHaveLength(0);
    // the new, empty sale owes nothing: the band does not keep the last ticket's total
    expect(screen.getByRole("status", { name: "À payer" })).toHaveTextContent("0,00");
    // the printer answers after the change was handed over
    await emit("print-failed", { documentId: "d-1", number: "TK2026-C1-000123", code: "printer_offline" });
    await user.click(await screen.findByRole("button", { name: "Réimprimer" }));
    await waitFor(() => expect(calls).toContain("hardware_retry_print"));
  });

  it("empties the cart and its return as soon as an exchange is committed", async () => {
    const exchanged: unknown[] = [];
    mockSale({
      returns_exchange: (args) => {
        exchanged.push(args);
        return {
          exchange: {
            returned: {
              document: { ...documentFixture, id: "d-ret", number: "AV2026-C1-000007" },
              refund: 0,
              storeCredit: 10_000,
            },
            sale: saleDoneFixture.sale,
            netToPay: 20_500,
          },
          printQueued: false,
        };
      },
    });
    const ret = {
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
    useCart.getState().startExchange({ input: ret, value: 10_000, originalNumber: "TK2026-C1-000100" });
    const { user, dialog } = await openPayment();
    await user.click(within(dialog).getByRole("button", { name: "Montant exact" }));
    await waitFor(() => expect(within(dialog).getByRole("button", { name: /Valider/ })).toBeEnabled());
    await user.click(within(dialog).getByRole("button", { name: /Valider/ }));
    expect(await screen.findByRole("dialog", { name: "Vente encaissée" })).toBeInTheDocument();
    expect(exchanged).toHaveLength(1);
    expect(exchanged[0]).toMatchObject({ request: { ret } });
    // neither the sold lines nor the return survive the commit: nothing to sell or refund twice
    expect(useCart.getState().lines).toHaveLength(0);
    expect(useCart.getState().exchange).toBeNull();
    expect(screen.getByText("TK2026-C1-000123")).toBeInTheDocument();
    expect(screen.getByText("Sans ticket imprimé.")).toBeInTheDocument();
  });

  it("shows no figure while a cheque is incomplete, then the backend's once it is filled in", async () => {
    const previews: unknown[] = [];
    mockSale({
      sales_preview: (args) => {
        previews.push(args);
        const tenders = (args.input as { tenders: (Tender & Record<string, unknown>)[] }).tenders;
        const incomplete = tenders.some(
          (p) => p.method === "cheque" && (!p.reference || !p.chequeBank || !p.chequeDate),
        );
        if (incomplete) {
          return Promise.reject({ code: "cheque_details_required", field: "payments[0].reference" });
        }
        return preview(args);
      },
    });
    const { user, dialog } = await openPayment();
    await user.click(within(dialog).getByRole("button", { name: "Chèque" }));
    expect(
      await within(dialog).findByText("Pour un chèque, saisissez son numéro, la banque et la date."),
    ).toBeInTheDocument();
    // the backend refused these tenders: no made-up zero, no "Rendu" telling the cashier it is settled
    expect(within(dialog).queryByRole("status", { name: "Rendu" })).not.toBeInTheDocument();
    expect(within(dialog).queryByText(/0,00/)).not.toBeInTheDocument();
    expect(within(dialog).getByRole("status", { name: "Reste à payer" })).toHaveTextContent("—");
    expect(within(dialog).getByRole("button", { name: /Valider/ })).toBeDisabled();

    await user.type(within(dialog).getByLabelText("N° du chèque"), "0012345");
    await user.type(within(dialog).getByLabelText("Banque"), "BNA");
    fireEvent.change(within(dialog).getByLabelText("Date du chèque"), { target: { value: "2026-09-29" } });
    await waitFor(() => expect(within(dialog).getByRole("button", { name: /Valider/ })).toBeEnabled());
    expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();
    expect(within(dialog).getByRole("status", { name: "Rendu" })).toHaveTextContent("0,00");
    expect(previews.at(-1)).toMatchObject({
      input: {
        tenders: [
          {
            method: "cheque",
            amount: 30_000,
            reference: "0012345",
            chequeBank: "BNA",
            chequeDate: "2026-09-29",
          },
        ],
      },
    });
  });

  it("sends one ticket when Enter is pressed twice", async () => {
    const created: unknown[] = [];
    mockSale({
      sales_create: (args) => {
        created.push(args);
        return saleDoneFixture;
      },
    });
    const { user, dialog } = await openPayment();
    await user.click(within(dialog).getByRole("button", { name: "Montant exact" }));
    await waitFor(() => expect(within(dialog).getByRole("button", { name: /Valider/ })).toBeEnabled());
    const amount = within(dialog).getByLabelText("Espèces");
    fireEvent.keyDown(amount, { code: "Enter", key: "Enter" });
    fireEvent.keyDown(amount, { code: "Enter", key: "Enter" });
    expect(await screen.findByRole("dialog", { name: "Vente encaissée" })).toBeInTheDocument();
    expect(created).toHaveLength(1);
  });

  it("lists the products short of stock when the shop blocks negative stock", async () => {
    mockSale({
      sales_create: () =>
        Promise.reject({
          code: "stock_insufficient",
          details: {
            shortages: [
              {
                productId: "p-milk",
                nameAr: "حليب",
                nameFr: "Lait Candia 1 L",
                nameEn: "Milk",
                locationId: "l",
                available: 1_000,
                needed: 2_000,
              },
            ],
          },
        }),
    });
    const { user, dialog } = await openPayment();
    await user.click(within(dialog).getByRole("button", { name: "Montant exact" }));
    await waitFor(() => expect(within(dialog).getByRole("button", { name: /Valider/ })).toBeEnabled());
    await user.click(within(dialog).getByRole("button", { name: /Valider/ }));
    expect(await within(dialog).findByText(/Lait Candia 1 L : 1 en stock, 2 demandés/)).toBeInTheDocument();
  });

  it("asks for Arabic only when the receipt prints none, its second language included (DOC-034)", async () => {
    const guarded: Array<{ lang: string; secondLang?: string | null }> = [];
    const guard = ({ input }: Record<string, unknown>) => {
      const asked = input as { lang: string; secondLang?: string | null };
      guarded.push(asked);
      return asked.lang === "ar" || asked.secondLang === "ar" ? null : { suggested: "ar", categoryIds: [] };
    };
    mockSale({ documents_arabic_guard: guard });
    const first = await openPayment();
    expect(await within(first.dialog).findByText(/devrait être imprimé en arabe/)).toBeInTheDocument();
    cleanup();
    useCart.getState().clear("fr");
    guarded.length = 0;
    // The shop prints bilingual receipts, French + Arabic (Réglages › Matériel).
    mockSale({
      documents_arabic_guard: guard,
      hardware_get: () => ({ ...printer, receipt: { ...printer.receipt, secondLang: "ar" } }),
    });
    const { dialog } = await openPayment();
    await waitFor(() => expect(guarded.at(-1)).toMatchObject({ lang: "fr", secondLang: "ar" }));
    expect(within(dialog).queryByText(/devrait être imprimé en arabe/)).not.toBeInTheDocument();
  });

  it("reads right to left in Arabic and prints in the chosen language", async () => {
    await setLanguage("ar");
    const created: unknown[] = [];
    mockSale({
      auth_session: () => ({ ...cashierSession, lang: "ar" }),
      sales_create: (args) => {
        created.push(args);
        return saleDoneFixture;
      },
    });
    const user = userEvent.setup();
    render(<App />);
    const field = await screen.findByLabelText("امسح أو ابحث عن منتج");
    await scanUntilLooked(field, "6130000000017", () => lookups);
    await screen.findByText("2 × Lait Candia 1 L");
    await user.keyboard("{F12}");
    const dialog = await screen.findByRole("dialog", { name: "القبض" });
    await user.click(within(dialog).getByRole("radio", { name: "العربية" }));
    await user.click(within(dialog).getByRole("button", { name: "المبلغ بالضبط" }));
    await waitFor(() => expect(within(dialog).getByRole("button", { name: /تأكيد/ })).toBeEnabled());
    await user.click(within(dialog).getByRole("button", { name: /تأكيد/ }));
    await waitFor(() => expect(created[0]).toMatchObject({ request: { sale: { cart: { lang: "ar" } } } }));
  });
});
