import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCart } from "@/features/pos/cart-store";
import { SaleScreen } from "@/features/pos/SaleScreen";
import { cashierSession, companyFixture, ownerTile, settingsValues } from "@/test/fixtures";
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
import { customerFixture, customerHitsFixture, promotionFixture } from "@/test/fixtures-m2";
import { linkedStatus } from "@/test/fixtures-m7";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";

// M1b's `sounds` module has no seam of its own: the module is the seam.
const beeps = vi.hoisted(() => [] as string[]);
vi.mock("@/lib/sounds", () => ({ beep: (kind: string) => beeps.push(kind) }));

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

type Tender = { method: string; amount: number };

/** The backend: 300,00 DA of milk, no duty; what is missing comes from the tenders; the cart's
 * customer comes back priced with the cart. */
function paying({ input }: Record<string, unknown>) {
  const { cart, tenders } = input as {
    cart: { customerId: string | null; lines: { productId: string | null; qty: number }[] };
    tenders: Tender[];
  };
  const base = previewFixture(30_000, 0, 0);
  const paid = tenders.reduce((sum, t) => sum + t.amount, 0);
  return {
    cart: {
      ...base.cart,
      lines: cart.lines.map((l, i) => ({ ...base.cart.lines[0]!, lineNo: i + 1, qty: l.qty })),
      customer:
        cart.customerId === customerFixture.id
          ? {
              id: customerFixture.id,
              code: customerFixture.code,
              name: customerFixture.name,
              phone: customerFixture.phone,
              balance: customerFixture.balance,
              creditLimit: customerFixture.creditLimit,
              availableCredit: customerFixture.availableCredit,
              storeCredit: customerFixture.storeCredit,
              isBlocked: false,
              discountBp: 0,
            }
          : null,
    },
    settlement: {
      ...base.settlement,
      nonCash: paid,
      paidTotal: paid,
      missing: Math.max(0, 30_000 - paid),
      change: 0,
      tenders,
    },
  };
}

function mockTill(extra: Record<string, Handler> = {}) {
  return mockCommands({
    auth_session: () => cashierSession,
    auth_list_users: () => [ownerTile, { ...ownerTile, id: "u-cashier", displayName: "Yacine" }],
    company_get: () => ({ ...companyFixture, defaultDocLang: "fr" }),
    settings_get: ({ key }) => settingsValues[String(key)] ?? null,
    hardware_get: () => hardware,
    cash_current: () => cashSessionFixture,
    catalog_categories_list: () => categoriesFixture,
    catalog_price_levels_list: () => priceLevelsFixture,
    catalog_tax_rates_list: () => taxRatesFixture,
    products_grid: () => productHitsFixture,
    products_search: () => productHitsFixture,
    products_find_by_code: () => scanHitFixture,
    sales_held: () => [],
    customers_search: () => customerHitsFixture,
    customers_get: () => customerFixture,
    sales_preview: paying,
    ...extra,
  });
}

/** Two milks already rung up. */
function withMilk() {
  useCart.getState().addProduct({
    productId: "p-milk",
    qty: 2_000,
    display: { name: "Lait Candia 1 L", unitCode: "pc", unitDecimals: 0, price: 15_000 },
  });
}

function renderTill() {
  return renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
}

/** F3, the name typed, the customer tapped. */
async function pickKarim(user: ReturnType<typeof userEvent.setup>) {
  await screen.findByLabelText("Scanner ou rechercher un produit");
  await user.keyboard("{F3}");
  await user.type(await screen.findByRole("searchbox", { name: "Client" }), "kar");
  await user.click(await screen.findByRole("button", { name: /Karim Benali/ }));
  return screen.findByRole("group", { name: "Client du ticket" });
}

describe("till with customers", () => {
  beforeEach(() => {
    useCart.getState().clear("fr");
    beeps.length = 0;
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(700);
  });

  it("credit above the limit asks for a manager PIN and beeps", async () => {
    let attempts = 0;
    mockTill({
      sales_create: () => {
        attempts += 1;
        return Promise.reject({
          code: "credit_limit_exceeded",
          details: { customerId: customerFixture.id, balance: 300_000, limit: 500_000, after: 800_000 },
        });
      },
    });
    withMilk();
    renderTill();
    await screen.findByLabelText("Scanner ou rechercher un produit");
    fireEvent.keyDown(window, { key: "F3" });
    fireEvent.change(await screen.findByRole("searchbox", { name: "Client" }), { target: { value: "kar" } });
    fireEvent.click(await screen.findByText("Karim Benali"));
    fireEvent.keyDown(window, { key: "F12" });
    const pay = await screen.findByRole("dialog", { name: "Encaissement" });
    await waitFor(() =>
      expect(within(pay).getByRole("status", { name: "Reste à payer" })).toHaveTextContent("300,00"),
    );
    fireEvent.click(within(pay).getByRole("button", { name: "Crédit client" }));
    const validate = within(pay).getByRole("button", { name: "Valider" });
    await waitFor(() => expect(validate).toBeEnabled());
    fireEvent.click(validate);
    const approval = await screen.findByRole("dialog", { name: /Accord d'un responsable/ });
    expect(within(approval).getByText(/Plafond dépassé/)).toBeInTheDocument();
    expect(within(approval).getByText("8 000,00", { exact: false })).toBeInTheDocument();
    expect(beeps).toContain("error");
    await waitFor(() => expect(attempts).toBe(1));
  });

  it("maps the M2 refusals to the permissions a manager approves", async () => {
    const { approvalPermission } = await import("@/lib/approval");
    expect(approvalPermission({ code: "credit_limit_exceeded" })).toBe("credit.limit_override");
    expect(approvalPermission({ code: "price_below_floor" })).toBe("sales.price_override");
  });

  it("finds a customer typed in Arabic-Indic digits, shows his account and sends him with the cart", async () => {
    const searches: unknown[] = [];
    const previews: unknown[] = [];
    mockTill({
      customers_search: (args) => {
        searches.push(args);
        return [customerHitsFixture[0], { ...customerHitsFixture[1], isBlocked: true }];
      },
      sales_preview: (args) => {
        previews.push(args);
        return paying(args);
      },
    });
    withMilk();
    const user = userEvent.setup();
    renderTill();
    await user.click(await screen.findByRole("button", { name: "Client" }));
    const sheet = await screen.findByRole("dialog", { name: "Choisir le client" });
    const search = within(sheet).getByRole("searchbox", { name: "Client" });
    await user.type(search, "٠٥٥٠");
    await waitFor(() => expect(searches.at(-1)).toMatchObject({ query: { text: "0550" } }));
    expect(within(sheet).getByRole("button", { name: /Nadia Kaci/ })).toHaveTextContent("Bloqué");
    // Esc clears what was typed first, the sheet stays open
    await user.keyboard("{Escape}");
    expect(search).toHaveValue("");
    expect(screen.getByRole("dialog", { name: "Choisir le client" })).toBeInTheDocument();
    const karim = within(sheet).getByRole("button", { name: /Karim Benali/ });
    expect(karim).toHaveTextContent("3 000,00");
    await user.click(karim);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const strip = await screen.findByRole("group", { name: "Client du ticket" });
    await waitFor(() => expect(strip).toHaveTextContent("Crédit disponible 2 000,00"));
    expect(strip).toHaveTextContent("Karim Benali");
    expect(strip).toHaveTextContent("Solde 3 000,00");
    await waitFor(() =>
      expect(previews.at(-1)).toMatchObject({ input: { cart: { customerId: "c-karim" } } }),
    );
    await user.click(within(strip).getByRole("button", { name: "Retirer le client" }));
    expect(screen.queryByRole("group", { name: "Client du ticket" })).toBeNull();
    await waitFor(() => expect(previews.at(-1)).toMatchObject({ input: { cart: { customerId: null } } }));
  });

  it("creates a customer from the till with his name and phone", async () => {
    const created: unknown[] = [];
    const nadia = { ...customerFixture, id: "c-new", code: "C00003", name: "Nadia", balance: 0 };
    mockTill({
      customers_search: () => [],
      customers_quick_create: (args) => {
        created.push(args);
        return nadia;
      },
      customers_get: () => nadia,
    });
    const user = userEvent.setup();
    renderTill();
    await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.keyboard("{F3}");
    const sheet = await screen.findByRole("dialog", { name: "Choisir le client" });
    await user.type(within(sheet).getByRole("searchbox", { name: "Client" }), "Nadia");
    await user.click(within(sheet).getByRole("button", { name: "Nouveau client" }));
    expect(within(sheet).getByLabelText("Nom")).toHaveValue("Nadia");
    await user.type(within(sheet).getByLabelText("Téléphone (facultatif)"), "٠٦٦١ ٢٣ ٤٥ ٦٧");
    await user.click(within(sheet).getByRole("button", { name: "Enregistrer le client" }));
    const strip = await screen.findByRole("group", { name: "Client du ticket" });
    expect(strip).toHaveTextContent("Nadia");
    expect(created).toEqual([{ name: "Nadia", phone: "0661 23 45 67" }]);
    expect(useCart.getState().customerId).toBe("c-new");
  });

  it("offers the account as a refund method when the ticket had a customer", async () => {
    const created: unknown[] = [];
    mockTill({
      documents_find: () => ({ ...documentFixture, customerId: customerFixture.id }),
      returns_returnable: () => [{ line: documentFixture.lines[0], returned: 0, remaining: 2_000 }],
      returns_quote: () => ({ value: 15_000, lines: [] }),
      returns_create: (args) => {
        created.push(args);
        return {
          returned: {
            document: {
              ...documentFixture,
              id: "d-br",
              docType: "return_note",
              number: "BR2026-C1-000004",
              customerId: customerFixture.id,
              totalDue: -15_000,
              payments: [{ ...documentFixture.payments[0]!, method: "credit", amount: -15_000 }],
            },
            refund: 15_000,
            storeCredit: 0,
          },
          printQueued: false,
        };
      },
    });
    const user = userEvent.setup();
    renderTill();
    await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.keyboard("{F8}");
    const dialog = await screen.findByRole("dialog", { name: "Retour de marchandise" });
    await user.type(
      within(dialog).getByLabelText("N° du ticket (scannez son code-barres)"),
      "TK2026-C1-000123{Enter}",
    );
    expect(await within(dialog).findByText(/Karim Benali/)).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText("Quantité rendue (Lait Candia 1 L)"), "1");
    await user.click(within(dialog).getByRole("radio", { name: "Crédit client" }));
    expect(within(dialog).getByRole("radio", { name: "Avoir client" })).toBeInTheDocument();
    expect(within(dialog).queryByLabelText("Référence du remboursement")).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Continuer" }));
    await user.click(await within(dialog).findByRole("button", { name: "Rembourser" }));
    const done = await screen.findByRole("dialog", { name: "Retour enregistré" });
    expect(within(done).getByRole("status", { name: "Porté au compte du client" })).toHaveTextContent(
      "150,00",
    );
    expect(created[0]).toMatchObject({
      request: { input: { originalId: "d-1", refundMethod: "credit", refundReference: null } },
    });
  });

  it("repeats the customer's last sale, priced again", async () => {
    const previews: unknown[] = [];
    const asked: unknown[] = [];
    mockTill({
      customers_documents: () => [
        { ...documentFixture, id: "d-br", docType: "return_note" },
        { ...documentFixture, id: "d-last" },
      ],
      documents_get: (args) => {
        asked.push(args);
        return {
          ...documentFixture,
          id: "d-last",
          customerId: customerFixture.id,
          lines: [
            { ...documentFixture.lines[0]!, qty: 3_000, unitPrice: 14_000, discountBp: 500 },
            { ...documentFixture.lines[0]!, id: "l-2", productId: null, description: "Sac" },
            { ...documentFixture.lines[0]!, id: "l-3", productId: "p-part", parentLineId: "l-1" },
          ],
        };
      },
      sales_preview: (args) => {
        previews.push(args);
        return paying(args);
      },
    });
    const user = userEvent.setup();
    renderTill();
    await pickKarim(user);
    await user.keyboard("{F3}");
    const sheet = await screen.findByRole("dialog", { name: "Choisir le client" });
    await user.click(within(sheet).getByRole("button", { name: "Reprendre son dernier achat" }));
    expect(await screen.findByText("3 × Lait Candia 1 L")).toBeInTheDocument();
    expect(asked).toEqual([{ id: "d-last" }]);
    await waitFor(() =>
      expect(previews.at(-1)).toMatchObject({
        input: {
          cart: {
            customerId: "c-karim",
            lines: [{ productId: "p-milk", qty: 3_000, unitPrice: null, discount: { kind: "none" } }],
          },
        },
      }),
    );
  });

  it("warns, never blocks, when DZBuild flags the chosen customer's number (DZB-015)", async () => {
    const asked: unknown[] = [];
    mockTill({
      cloud_status: () => linkedStatus,
      cloud_customer_flag: (args) => {
        asked.push(args);
        return { isBanned: false, fraudScore: 80 };
      },
    });
    const user = userEvent.setup();
    renderTill();
    await pickKarim(user);
    await user.keyboard("{F3}");
    const sheet = await screen.findByRole("dialog", { name: "Choisir le client" });
    expect(await within(sheet).findByRole("status", { name: "Acheteur signalé" })).toBeInTheDocument();
    expect(asked).toContainEqual({ phone: customerFixture.phone });
    // a warning only: the customer stays chosen and his last sale can still be rung up
    expect(within(sheet).getByRole("button", { name: "Reprendre son dernier achat" })).toBeEnabled();
  });

  it("applies a named discount to the line in one tap", async () => {
    const previews: unknown[] = [];
    mockTill({
      promotions_presets: () => [promotionFixture],
      sales_preview: (args) => {
        previews.push(args);
        return paying(args);
      },
    });
    withMilk();
    const user = userEvent.setup();
    renderTill();
    await user.click(await screen.findByText("2 × Lait Candia 1 L"));
    const edit = await screen.findByRole("dialog", { name: "Modifier la ligne" });
    await user.click(await within(edit).findByRole("button", { name: /Remise fidèle/ }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() =>
      expect(previews.at(-1)).toMatchObject({
        input: { cart: { lines: [{ productId: "p-milk", discount: { kind: "percent", value: 500 } }] } },
      }),
    );
  });

  it("names the automatic discount of each line", async () => {
    mockTill({
      sales_preview: (args) => {
        const out = paying(args);
        const line = out.cart.lines[0]!;
        const discounted = {
          ...line,
          amounts: { ...line.amounts, discountAmount: 3_000 },
        };
        return {
          ...out,
          cart: {
            ...out.cart,
            lines: [
              {
                ...discounted,
                autoDiscount: {
                  source: "promotion",
                  promotionId: "p1",
                  label: "Soldes d'été",
                  amount: 3_000,
                },
              },
              {
                ...discounted,
                lineNo: 2,
                autoDiscount: { source: "customer", promotionId: null, label: null, amount: 3_000 },
              },
            ],
          },
        };
      },
    });
    withMilk();
    useCart.getState().addProduct({
      productId: "p-oil",
      qty: 1_000,
      display: { name: "Huile Elio 5 L", unitCode: "pc", unitDecimals: 0, price: 65_000 },
    });
    renderTill();
    const cart = await screen.findByRole("list", { name: "Ticket" });
    expect(await within(cart).findByText("Soldes d'été")).toBeInTheDocument();
    expect(within(cart).getByText("Remise client")).toBeInTheDocument();
  });

  it("pays with store credit up to what the customer has, and says why the backend refused", async () => {
    const created: unknown[] = [];
    mockTill({
      customers_get: () => ({
        ...customerFixture,
        balance: -10_000,
        availableCredit: 500_000,
        storeCredit: 10_000,
      }),
      sales_create: (args) => {
        created.push(args);
        return Promise.reject({ code: "store_credit_insufficient", details: { available: 5_000 } });
      },
    });
    withMilk();
    useCart.getState().setCustomer(customerFixture.id);
    const user = userEvent.setup();
    renderTill();
    await screen.findByRole("group", { name: "Client du ticket" });
    await user.keyboard("{F12}");
    const pay = await screen.findByRole("dialog", { name: "Encaissement" });
    expect(await within(pay).findByText(/Karim Benali/)).toBeInTheDocument();
    await waitFor(() =>
      expect(within(pay).getByRole("status", { name: "Reste à payer" })).toHaveTextContent("300,00"),
    );
    await user.click(within(pay).getByRole("button", { name: "Avoir client" }));
    expect(within(pay).getByLabelText("Avoir client")).toHaveValue("100,00");
    expect(within(pay).queryByLabelText(/N° du bon/)).toBeNull();
    await waitFor(() =>
      expect(within(pay).getByRole("status", { name: "Reste à payer" })).toHaveTextContent("200,00"),
    );
    await user.click(within(pay).getByRole("button", { name: "Montant exact" }));
    await waitFor(() => expect(within(pay).getByRole("button", { name: "Valider" })).toBeEnabled());
    await user.click(within(pay).getByRole("button", { name: "Valider" }));
    expect(await within(pay).findByRole("alert")).toHaveTextContent(
      "L'avoir du client ne couvre pas ce montant",
    );
    expect(created[0]).toMatchObject({
      request: {
        sale: {
          cart: { customerId: "c-karim" },
          payments: [
            { method: "cash", amount: 20_000 },
            { method: "voucher", amount: 10_000, reference: null },
          ],
        },
      },
    });
  });

  it("shows the customer and his balance after the sale", async () => {
    mockTill({
      sales_create: () => ({
        ...saleDoneFixture,
        sale: {
          ...saleDoneFixture.sale,
          document: { ...documentFixture, customerId: customerFixture.id },
          customerBalance: 330_000,
        },
      }),
    });
    withMilk();
    useCart.getState().setCustomer(customerFixture.id);
    const user = userEvent.setup();
    renderTill();
    await screen.findByRole("group", { name: "Client du ticket" });
    await user.keyboard("{F12}");
    const pay = await screen.findByRole("dialog", { name: "Encaissement" });
    await waitFor(() =>
      expect(within(pay).getByRole("status", { name: "Reste à payer" })).toHaveTextContent("300,00"),
    );
    await user.click(within(pay).getByRole("button", { name: "Crédit client" }));
    await waitFor(() => expect(within(pay).getByRole("button", { name: "Valider" })).toBeEnabled());
    await user.click(within(pay).getByRole("button", { name: "Valider" }));
    const done = await screen.findByRole("dialog", { name: "Vente encaissée" });
    expect(await within(done).findByText("Karim Benali")).toBeInTheDocument();
    expect(within(done).getByText("Solde du client après ce ticket").closest("p")).toHaveTextContent(
      "3 300,00",
    );
    expect(useCart.getState().customerId).toBeNull();
  });
});
