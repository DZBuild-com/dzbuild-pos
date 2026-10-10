import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Toaster } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCart } from "@/features/pos/cart-store";
import { SaleScreen } from "@/features/pos/SaleScreen";
import { setLanguage } from "@/lib/i18n";
import { SCAN_MAX_GAP_MS, setBurstGuard } from "@/lib/scanner";
import { cashierSession, companyFixture, ownerTile, settingsValues } from "@/test/fixtures";
import {
  cashSessionFixture,
  categoriesFixture,
  heldCartFixture,
  previewFixture,
  priceLevelsFixture,
  productHitsFixture,
  saleDoneFixture,
  scanHitFixture,
  stockLevelsFixture,
  taxRatesFixture,
} from "@/test/fixtures-m1";
import { lanTillStatusFixture } from "@/test/fixtures-m5";
import type { ProductHit } from "@/lib/ipc/types";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";
import { scanDigits } from "@/test/scan";

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

const NAMES: Record<string, Record<string, string>> = {
  fr: { "p-milk": "Lait Candia 1 L", "p-oil": "Huile Elio 5 L" },
  ar: { "p-milk": "حليب كانديا 1 لتر", "p-oil": "زيت إليو 5 لتر" },
};

/** The backend's answer for the cart sent: one priced line per cart line, names in the cart's language. */
function echoPreview({ input }: Record<string, unknown>) {
  const base = previewFixture(30_000, 0);
  const cart = (input as { cart: { lang: string; lines: { productId: string | null; qty: number }[] } }).cart;
  const template = base.cart.lines[0]!;
  return {
    ...base,
    cart: {
      ...base.cart,
      lines: cart.lines.map((l, i) => ({
        ...template,
        lineNo: i + 1,
        productId: l.productId,
        qty: l.qty,
        description: NAMES[cart.lang]?.[l.productId ?? ""] ?? "?",
      })),
    },
  };
}

function mockPos(extra: Record<string, Handler> = {}) {
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
    products_search: () => productHitsFixture.slice(0, 1),
    products_find_by_code: ({ code }) => (code === "6130000000017" ? scanHitFixture : null),
    sales_preview: echoPreview,
    sales_held: () => [],
    ...extra,
  });
}

function scan(target: Element, code: string) {
  scanDigits(target, code);
}

/** What a French AZERTY Windows makes of a scanner emulating a US keyboard: the top-row keys
 * without Shift, so "6" arrives as "-" (the physical key Digit6). */
const AZERTY_TOP_ROW: Record<string, string> = {
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

function scanOnAzerty(target: Element, code: string) {
  scanDigits(target, code, (digit) => AZERTY_TOP_ROW[digit] ?? digit);
}

/** The backend once the exact amount is typed: everything paid in cash, nothing missing. */
function paidPreview(args: Record<string, unknown>) {
  const tenders = (args.input as { tenders: { amount: number }[] }).tenders;
  const paid = tenders.reduce((sum, t) => sum + t.amount, 0);
  return { ...echoPreview(args), settlement: previewFixture(30_000, paid).settlement };
}

const voider = {
  ...cashierSession,
  permissions: [...cashierSession.permissions, "sales.void_line" as const],
};

describe("SaleScreen", () => {
  beforeEach(() => {
    useCart.getState().clear("fr");
    // jsdom has no layout: TanStack Virtual measures the grid's scroller (offsetWidth/offsetHeight)
    // on mount, replacing `initialRect`, and would render no tile row at 0×0.
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(700);
  });

  it("a scan adds the product once even when focus is on a button", async () => {
    const calls = mockPos();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    const discount = await screen.findByRole("button", { name: /Remise/ });
    discount.focus();
    scan(discount, "6130000000017");
    await waitFor(() => expect(calls.filter((c) => c === "products_find_by_code")).toHaveLength(1));
    const cart = screen.getByRole("list", { name: "Ticket" });
    expect(await within(cart).findByText("1 × Lait Candia 1 L")).toBeInTheDocument();
    expect(screen.getByText("Dernier article : Lait Candia 1 L")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows the amount due from the backend in the brand band", async () => {
    mockPos();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    scan(await screen.findByLabelText("Scanner ou rechercher un produit"), "6130000000017");
    const band = await screen.findByRole("status", { name: "À payer" });
    await waitFor(() => expect(band).toHaveTextContent("305,00"));
  });

  it("adds a typed code with Enter and explains an unknown one", async () => {
    mockPos();
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.type(field, "0000{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("Aucun produit pour le code « 0000 »");
    await user.clear(field);
    await user.type(field, "6130000000017{Enter}");
    expect(await screen.findByText("1 × Lait Candia 1 L")).toBeInTheDocument();
    expect(field).toHaveValue("");
  });

  it("3* then a scan sells three", async () => {
    mockPos();
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.type(field, "3*");
    // `*` acts once no scanner key follows it (it could be inside a burst).
    expect(await screen.findByText("× 3")).toBeInTheDocument();
    // The cashier picks the item up: the scan comes after a pause, so the scanner never takes the
    // typed "3*" for the start of its burst (user-event types faster than SCAN_MAX_GAP_MS).
    await new Promise((resolve) => setTimeout(resolve, SCAN_MAX_GAP_MS * 2));
    scan(field, "6130000000017");
    expect(await screen.findByText("3 × Lait Candia 1 L")).toBeInTheDocument();
  });

  it("+ and - change the selected line, - and Delete are voids a manager approves", async () => {
    const events: unknown[] = [];
    // The backend: a void needs sales.void_line, which one approval grants for one operation.
    let approved = false;
    mockPos({
      sales_record_event: (args) => {
        events.push(args);
        if (!approved) {
          return Promise.reject({
            code: "insufficient_permission",
            details: { permission: "sales.void_line" },
          });
        }
        approved = false;
        return null;
      },
      auth_manager_override: () => {
        approved = true;
        return {
          approverId: "u-owner",
          approverName: "Amine Benali",
          permission: "sales.void_line",
          grantedAt: 1,
          discountLimitBp: 10_000,
        };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    scan(field, "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    field.focus();
    await user.keyboard("+");
    expect(await screen.findByText("2 × Lait Candia 1 L")).toBeInTheDocument();
    // One unit less is a partial void (SAL-040): journaled, and a manager approves it.
    await user.keyboard("-");
    await user.click(await screen.findByRole("button", { name: /Amine Benali/ }));
    await user.keyboard("2580");
    await user.click(screen.getByRole("button", { name: "Autoriser" }));
    expect(await screen.findByText("1 × Lait Candia 1 L")).toBeInTheDocument();
    expect(events.at(-1)).toEqual({
      event: {
        kind: "qty_reduced",
        index: 0,
        qty: 1_000,
        cart: expect.objectContaining({
          lines: [expect.objectContaining({ productId: "p-milk", qty: 2_000 })],
        }),
      },
    });
    field.focus();
    await user.keyboard("{Delete}");
    await user.click(await screen.findByRole("button", { name: /Amine Benali/ }));
    await user.keyboard("2580");
    await user.click(screen.getByRole("button", { name: "Autoriser" }));
    await waitFor(() => expect(screen.queryByText("1 × Lait Candia 1 L")).toBeNull());
    // The cart on screen and the line: the name and the amount are the backend's (AUD-001).
    expect(events.at(-1)).toEqual({
      event: {
        kind: "line_voided",
        index: 0,
        cart: expect.objectContaining({
          lines: [expect.objectContaining({ productId: "p-milk", qty: 1_000 })],
        }),
      },
    });
  });

  it("a scan starting with 6 on an AZERTY Windows never lowers the selected line", async () => {
    // Every Algerian EAN-13 starts with 613: its first key reaches the scan field as "-".
    mockPos({ sales_record_event: () => null });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    const oil = await screen.findByRole("button", { name: /Huile Elio 5 L/ });
    await user.click(oil);
    await user.click(oil);
    expect(await screen.findByText("2 × Huile Elio 5 L")).toBeInTheDocument();
    const field = screen.getByLabelText("Scanner ou rechercher un produit");
    field.focus();
    scanOnAzerty(field, "6130000000017");
    expect(await screen.findByText("1 × Lait Candia 1 L")).toBeInTheDocument();
    await new Promise((resolve) => setTimeout(resolve, SCAN_MAX_GAP_MS * 4));
    expect(screen.getByText("2 × Huile Elio 5 L")).toBeInTheDocument();
    // The same key pressed alone by a person still lowers the selected line.
    fireEvent.keyDown(field, { code: "ArrowUp", key: "ArrowUp" });
    fireEvent.keyDown(field, { code: "Digit6", key: "-" });
    expect(await screen.findByText("1 × Huile Elio 5 L")).toBeInTheDocument();
  });

  it("a void journals the cart as it stands, and a second Delete waits for the first", async () => {
    const events: unknown[] = [];
    let release = () => {};
    mockPos({
      sales_record_event: (args) => {
        events.push(args);
        return new Promise((resolve) => {
          release = () => resolve(null);
        });
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={voider} />, { route: "/pos" });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    scan(field, "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    await user.click(await screen.findByRole("button", { name: /Huile Elio 5 L/ }));
    await screen.findByText("1 × Huile Elio 5 L");
    field.focus();
    await user.keyboard("{ArrowUp}{Delete}{Delete}");
    await waitFor(() => expect(events).toHaveLength(1));
    const lines = (...ids: string[]) => ids.map((productId) => expect.objectContaining({ productId }));
    expect(events[0]).toEqual({
      event: {
        kind: "line_voided",
        index: 0,
        cart: expect.objectContaining({ lines: lines("p-milk", "p-oil") }),
      },
    });
    await act(async () => release());
    await waitFor(() => expect(screen.queryByText("1 × Lait Candia 1 L")).toBeNull());
    expect(screen.getByText("1 × Huile Elio 5 L")).toBeInTheDocument();
    expect(events).toHaveLength(1);
    // Right after, before any new preview: the event carries the cart without the voided line.
    await user.keyboard("{Delete}");
    await waitFor(() => expect(events).toHaveLength(2));
    expect(events[1]).toEqual({
      event: { kind: "line_voided", index: 0, cart: expect.objectContaining({ lines: lines("p-oil") }) },
    });
  });

  it("shows the subtotal before the discount, from the backend", async () => {
    const discounted = previewFixture(27_000, 0);
    discounted.cart.totals = { ...discounted.cart.totals, grossTotal: 30_000, discountTotal: 3_000 };
    mockPos({ sales_preview: () => discounted });
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    scan(await screen.findByLabelText("Scanner ou rechercher un produit"), "6130000000017");
    const totals = await screen.findByRole("group", { name: "Totaux" });
    await waitFor(() =>
      expect(within(totals).getByText("Sous-total").nextSibling).toHaveTextContent("300,00"),
    );
    expect(within(totals).getByText("Remise").nextSibling).toHaveTextContent("-30,00");
    expect(screen.getByRole("status", { name: "À payer" })).toHaveTextContent("275,00");
  });

  it("filters the grid by category and adds a tile", async () => {
    const grids: unknown[] = [];
    mockPos({
      products_grid: (args) => {
        grids.push(args);
        return productHitsFixture;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await user.click(await screen.findByRole("radio", { name: "Crèmerie" }));
    await waitFor(() => expect(grids).toContainEqual({ categoryId: "c-dairy", limit: 200 }));
    await user.click(await screen.findByRole("button", { name: /Huile Elio 5 L/ }));
    expect(await screen.findByText("1 × Huile Elio 5 L")).toBeInTheDocument();
  });

  it("asks to open the till before selling", async () => {
    mockPos({ cash_current: () => null });
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    expect(await screen.findByText("La caisse est fermée")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ouvrir la caisse" })).toBeInTheDocument();
  });

  it("undoes a hold from its toast: the ticket comes back to the cart", async () => {
    const recalls: unknown[] = [];
    mockPos({
      sales_hold: () => heldCartFixture,
      sales_recall: (args) => {
        recalls.push(args);
        return { cart: heldCartFixture.cart, priced: previewFixture(30_000, 0).cart, changedLines: [] };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(
      <>
        <SaleScreen session={cashierSession} />
        <Toaster />
      </>,
      { route: "/pos" },
    );
    scan(await screen.findByLabelText("Scanner ou rechercher un produit"), "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    await user.keyboard("{F6}");
    const hold = await screen.findByRole("dialog", { name: "Mettre en attente" });
    await user.click(within(hold).getByRole("button", { name: "Mettre en attente" }));
    await waitFor(() => expect(screen.queryByText("1 × Lait Candia 1 L")).toBeNull());
    // The hold dialog is gone by now: the undo must not depend on it.
    await user.click(await screen.findByRole("button", { name: "Annuler" }));
    expect(await screen.findByText("2 × Lait Candia 1 L")).toBeInTheDocument();
    expect(recalls).toEqual([{ id: heldCartFixture.id }]);
  });

  it("refuses to undo a hold into a cart that has lines", async () => {
    const calls = mockPos({
      sales_hold: () => heldCartFixture,
      sales_recall: () => ({
        cart: heldCartFixture.cart,
        priced: previewFixture(30_000, 0).cart,
        changedLines: [],
      }),
    });
    const user = userEvent.setup();
    renderWithProviders(
      <>
        <SaleScreen session={cashierSession} />
        <Toaster />
      </>,
      { route: "/pos" },
    );
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    scan(field, "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    await user.keyboard("{F6}");
    const hold = await screen.findByRole("dialog", { name: "Mettre en attente" });
    await user.click(within(hold).getByRole("button", { name: "Mettre en attente" }));
    await waitFor(() => expect(screen.queryByText("1 × Lait Candia 1 L")).toBeNull());
    // The next customer's first item is already in the cart: a recall would replace it unjournaled.
    scan(field, "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    await user.click(await screen.findByRole("button", { name: "Annuler" }));
    expect(
      await screen.findByText("Mettez d'abord le ticket en cours en attente ou encaissez-le."),
    ).toBeInTheDocument();
    expect(calls).not.toContain("sales_recall");
    expect(screen.getByText("1 × Lait Candia 1 L")).toBeInTheDocument();
  });

  it("holds the ticket with F6 and recalls it with F7", async () => {
    const holds: unknown[] = [];
    mockPos({
      sales_hold: (args) => {
        holds.push(args);
        return heldCartFixture;
      },
      sales_held: () => [heldCartFixture],
      sales_recall: () => ({
        cart: heldCartFixture.cart,
        priced: previewFixture(30_000, 0).cart,
        changedLines: [1],
      }),
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    scan(await screen.findByLabelText("Scanner ou rechercher un produit"), "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    await user.keyboard("{F6}");
    const hold = await screen.findByRole("dialog", { name: "Mettre en attente" });
    await user.clear(within(hold).getByLabelText("Nom du ticket (client, table…)"));
    await user.type(within(hold).getByLabelText("Nom du ticket (client, table…)"), "Table 3");
    await user.click(within(hold).getByRole("button", { name: "Mettre en attente" }));
    await waitFor(() => expect(holds).toHaveLength(1));
    expect(holds[0]).toMatchObject({ label: "Table 3" });
    await waitFor(() => expect(screen.queryByText("1 × Lait Candia 1 L")).toBeNull());
    await user.keyboard("{F7}");
    const sheet = await screen.findByRole("dialog", { name: "Tickets en attente" });
    await user.click(within(sheet).getByRole("button", { name: "Reprendre Table 3" }));
    expect(await screen.findByText("2 × Lait Candia 1 L")).toBeInTheDocument();
    expect(screen.getByText("Le prix a changé sur 1 ligne depuis la mise en attente.")).toBeInTheDocument();
  });

  it("a discount above the cashier's right asks a manager (F5)", async () => {
    let approved = false;
    mockPos({
      sales_preview: ({ input }) => {
        const discounted = (input as { cart: { lines: { discount: { kind: string } }[] } }).cart.lines.some(
          (l) => l.discount.kind !== "none",
        );
        return discounted && !approved
          ? Promise.reject({ code: "discount_not_allowed" })
          : previewFixture(27_000, 0);
      },
      auth_manager_override: () => {
        approved = true;
        return {
          approverId: "u-owner",
          approverName: "Amine Benali",
          permission: "sales.discount",
          grantedAt: 1,
        };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    scan(await screen.findByLabelText("Scanner ou rechercher un produit"), "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    await user.keyboard("{F5}");
    const dialog = await screen.findByRole("dialog", { name: "Remise" });
    await user.type(within(dialog).getByLabelText("Valeur"), "10");
    await user.click(within(dialog).getByRole("button", { name: "Appliquer la remise" }));
    await user.click(await screen.findByRole("button", { name: "Demander l'accord d'un responsable" }));
    await user.click(await screen.findByRole("button", { name: /Amine Benali/ }));
    await user.keyboard("2580");
    await user.click(screen.getByRole("button", { name: "Autoriser" }));
    await waitFor(() => expect(screen.getByRole("status", { name: "À payer" })).toHaveTextContent("275,00"));
  });

  it("checks a price without selling (F4) and opens the drawer with a reason (F9)", async () => {
    const drawers: unknown[] = [];
    mockPos({
      stock_levels: () => stockLevelsFixture,
      hardware_open_drawer: (args) => {
        drawers.push(args);
        return null;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.keyboard("{F4}");
    const check = await screen.findByRole("dialog", { name: "Vérifier un prix" });
    await user.type(within(check).getByLabelText("Code-barres ou référence"), "6130000000017{Enter}");
    expect(await within(check).findByText("Lait Candia 1 L")).toBeInTheDocument();
    expect(within(check).getByText("24")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(useCart.getState().lines).toHaveLength(0);
    await user.keyboard("{F9}");
    const drawer = await screen.findByRole("dialog", { name: "Ouvrir le tiroir" });
    await user.click(within(drawer).getByRole("radio", { name: "Faire de la monnaie" }));
    await user.click(within(drawer).getByRole("button", { name: "Ouvrir le tiroir" }));
    await waitFor(() => expect(drawers).toEqual([{ input: { reason: "change_request", note: null } }]));
  });

  it("cancels the whole sale from the menu, journaled", async () => {
    const events: unknown[] = [];
    mockPos({
      sales_record_event: (args) => {
        events.push(args);
        return null;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(
      <SaleScreen
        session={{ ...cashierSession, permissions: [...cashierSession.permissions, "sales.void_line"] }}
      />,
      { route: "/pos" },
    );
    scan(await screen.findByLabelText("Scanner ou rechercher un produit"), "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    await user.keyboard("{Escape}");
    const menu = await screen.findByRole("dialog", { name: "Menu" });
    await user.click(within(menu).getByRole("button", { name: "Annuler la vente" }));
    await user.click(within(menu).getByRole("button", { name: "Confirmer l'annulation" }));
    await waitFor(() =>
      expect(events).toEqual([
        {
          event: {
            kind: "cart_cancelled",
            cart: expect.objectContaining({
              lines: [expect.objectContaining({ productId: "p-milk", qty: 1_000 })],
            }),
          },
        },
      ]),
    );
    expect(useCart.getState().lines).toHaveLength(0);
  });

  it("a scan while the payment is open never validates the sale nor fills a field", async () => {
    const calls = mockPos({ sales_create: () => saleDoneFixture, sales_preview: paidPreview });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    scan(await screen.findByLabelText("Scanner ou rechercher un produit"), "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    await user.keyboard("{F12}");
    const pay = await screen.findByRole("dialog", { name: "Encaissement" });
    await user.click(within(pay).getByRole("button", { name: "Montant exact" }));
    await waitFor(() => expect(within(pay).getByRole("button", { name: /Valider/ })).toBeEnabled());
    // the cashier scans the next item: its Enter would validate the sale
    scan(document.activeElement ?? pay, "6130000000017");
    await new Promise((resolve) => setTimeout(resolve, SCAN_MAX_GAP_MS * 2));
    expect(calls).not.toContain("sales_create");
    expect(calls.filter((c) => c === "products_find_by_code")).toHaveLength(1);
    expect(screen.getByRole("dialog", { name: "Encaissement" })).toBeInTheDocument();
  });

  it("a scan on the change screen starts the next sale with that item", async () => {
    const calls = mockPos({ sales_create: () => saleDoneFixture, sales_preview: paidPreview });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    scan(await screen.findByLabelText("Scanner ou rechercher un produit"), "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    await user.keyboard("{F12}");
    const pay = await screen.findByRole("dialog", { name: "Encaissement" });
    await user.click(within(pay).getByRole("button", { name: "Montant exact" }));
    await waitFor(() => expect(within(pay).getByRole("button", { name: /Valider/ })).toBeEnabled());
    await user.click(within(pay).getByRole("button", { name: /Valider/ }));
    const done = await screen.findByRole("dialog", { name: "Vente encaissée" });
    scan(done, "6130000000017");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Vente encaissée" })).toBeNull());
    expect(await screen.findByText("1 × Lait Candia 1 L")).toBeInTheDocument();
    expect(calls.filter((c) => c === "sales_create")).toHaveLength(1);
  });

  it("after a tap on a cart line, the keyboard acts on it (focus back in the scan field)", async () => {
    const events: unknown[] = [];
    mockPos({
      sales_record_event: (args) => {
        events.push(args);
        return null;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={voider} />, { route: "/pos" });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    scan(field, "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    await user.click(await screen.findByRole("button", { name: /Huile Elio 5 L/ }));
    await screen.findByText("1 × Huile Elio 5 L");
    await user.click(screen.getByText("1 × Lait Candia 1 L"));
    await waitFor(() => expect(field).toHaveFocus());
    await user.keyboard("{Delete}");
    await waitFor(() => expect(events).toHaveLength(1));
    expect(events[0]).toMatchObject({ event: { kind: "line_voided", index: 0 } });
    await waitFor(() => expect(screen.queryByText("1 × Lait Candia 1 L")).toBeNull());
  });

  it("a tap on the selected line opens it, and a lower quantity is journaled first", async () => {
    const events: unknown[] = [];
    mockPos({
      sales_record_event: (args) => {
        events.push(args);
        return null;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={voider} />, { route: "/pos" });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    scan(field, "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    field.focus();
    await user.keyboard("+");
    const line = await screen.findByText("2 × Lait Candia 1 L");
    // the scanned line is the selected one: one tap opens it (spec §10.5)
    await user.click(line);
    let edit = await screen.findByRole("dialog", { name: "Modifier la ligne" });
    // it can always be left with a button (touch screens have no Esc)
    await user.click(within(edit).getByRole("button", { name: "Annuler" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await user.click(screen.getByText("2 × Lait Candia 1 L"));
    edit = await screen.findByRole("dialog", { name: "Modifier la ligne" });
    expect(within(edit).getByRole("button", { name: "Fermer" })).toBeInTheDocument();
    const qty = within(edit).getByLabelText("Quantité");
    await user.clear(qty);
    await user.type(qty, "1");
    await user.click(within(edit).getByRole("button", { name: "Enregistrer la ligne" }));
    expect(await screen.findByText("1 × Lait Candia 1 L")).toBeInTheDocument();
    expect(events).toEqual([
      {
        event: {
          kind: "qty_reduced",
          index: 0,
          qty: 1_000,
          cart: expect.objectContaining({
            lines: [expect.objectContaining({ productId: "p-milk", qty: 2_000 })],
          }),
        },
      },
    ]);
  });

  it("refuses to hold or recall a ticket while an exchange is pending", async () => {
    const calls = mockPos({ sales_held: () => [heldCartFixture] });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    scan(await screen.findByLabelText("Scanner ou rechercher un produit"), "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    act(() =>
      useCart.getState().startExchange({
        input: {
          originalId: "d-1",
          lines: [],
          refundMethod: "cash",
          refundReference: null,
          lang: "fr",
          note: null,
        },
        value: 15_000,
        originalNumber: "TK2026-C1-000123",
      }),
    );
    await user.keyboard("{F6}");
    expect(await screen.findByText("Terminez ou annulez l'échange d'abord.")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).toBeNull();
    await user.keyboard("{F7}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls).not.toContain("sales_hold");
  });

  it("throwing a held ticket away is a cancellation a manager approves", async () => {
    let approved = false;
    const discards: unknown[] = [];
    mockPos({
      sales_held: () => [heldCartFixture],
      sales_discard_held: (args) => {
        discards.push(args);
        if (!approved) {
          return Promise.reject({
            code: "insufficient_permission",
            details: { permission: "sales.void_line" },
          });
        }
        return null;
      },
      auth_manager_override: () => {
        approved = true;
        return {
          approverId: "u-owner",
          approverName: "Amine Benali",
          permission: "sales.void_line",
          grantedAt: 1,
          discountLimitBp: 10_000,
        };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.keyboard("{F7}");
    const sheet = await screen.findByRole("dialog", { name: "Tickets en attente" });
    await user.click(within(sheet).getByRole("button", { name: "Supprimer" }));
    await user.click(within(sheet).getByRole("button", { name: "Confirmer la suppression" }));
    await user.click(await screen.findByRole("button", { name: /Amine Benali/ }));
    await user.keyboard("2580");
    await user.click(screen.getByRole("button", { name: "Autoriser" }));
    await waitFor(() => expect(discards).toHaveLength(2));
    expect(discards[1]).toEqual({ id: heldCartFixture.id });
  });

  it("reads a price-check scan from the physical keys on an AZERTY Windows (F4)", async () => {
    setBurstGuard(true);
    const codes: unknown[] = [];
    mockPos({
      stock_levels: () => stockLevelsFixture,
      products_find_by_code: ({ code }) => {
        codes.push(code);
        return code === "6130000000017" ? scanHitFixture : null;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.keyboard("{F4}");
    const check = await screen.findByRole("dialog", { name: "Vérifier un prix" });
    const field = within(check).getByLabelText("Code-barres ou référence");
    scanOnAzerty(field, "6130000000017");
    expect(await within(check).findByText("Lait Candia 1 L")).toBeInTheDocument();
    expect(codes).toEqual(["6130000000017"]);
    expect(field).toHaveValue("6130000000017");
    expect(useCart.getState().lines).toHaveLength(0);
  });

  it("looks a code unknown on this till up on the main PC", async () => {
    const searched: unknown[] = [];
    const calls = mockPos({
      lan_status: () => lanTillStatusFixture,
      products_find_by_code: () => null,
      lan_search_host: ({ query }) => {
        searched.push(query);
        return productHitsFixture.slice(0, 1);
      },
      lan_sync_now: () => null,
    });
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    await waitFor(() => expect(calls).toContain("lan_status"));
    scan(field, "6130000000099");
    expect(
      await screen.findByText(
        "Trouvé sur le poste principal : l'article arrive sur ce poste dans un instant.",
      ),
    ).toBeInTheDocument();
    expect(searched).toEqual([{ text: "6130000000099", limit: 5, categoryId: null, includeArchived: false }]);
    await waitFor(() => expect(calls).toContain("lan_sync_now"));
  });

  it("says a code is unknown everywhere when the main PC has nothing either", async () => {
    mockPos({
      lan_status: () => lanTillStatusFixture,
      products_find_by_code: () => null,
      lan_search_host: () => [],
    });
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    // the till's link is known (in the app, since sign-in): its pill shows in the banner
    expect(await screen.findByRole("status", { name: "Connecté" })).toBeInTheDocument();
    scan(field, "6130000000099");
    expect(await screen.findByText("Introuvable, même sur le poste principal.")).toBeInTheDocument();
  });

  it("works right to left in Arabic with Arabic product names", async () => {
    await setLanguage("ar");
    mockPos({ company_get: () => ({ ...companyFixture, defaultDocLang: "ar" }) });
    renderWithProviders(<SaleScreen session={{ ...cashierSession, lang: "ar" }} />, { route: "/pos" });
    const field = await screen.findByLabelText("امسح أو ابحث عن منتج");
    expect(document.documentElement).toHaveAttribute("dir", "rtl");
    expect(field).toHaveAttribute("dir", "auto");
    expect(await screen.findByRole("button", { name: /زيت إليو 5 لتر/ })).toBeInTheDocument();
    await act(async () => scan(field, "6130000000017"));
    expect(await screen.findByText("1 × حليب كانديا 1 لتر")).toBeInTheDocument();
  });
});
/** A promise the test settles by hand. */
function deferred<T>() {
  let resolve: (value: T) => void = () => {};
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

const [milkHit, oilHit] = productHitsFixture as [ProductHit, ProductHit];

/** The backend's search: the products with a word of the French name or the reference starting
 * with the text. */
function searchByName({ query }: Record<string, unknown>) {
  const text = (query as { text: string }).text.toLowerCase();
  return productHitsFixture.filter((h) =>
    [...h.nameFr.toLowerCase().split(" "), h.reference?.toLowerCase() ?? ""].some((w) => w.startsWith(text)),
  );
}

const cartNames = () => useCart.getState().lines.map((d) => [d.display.name, d.line.qty]);

describe("SaleScreen: searching a product by name", () => {
  beforeEach(() => {
    useCart.getState().clear("fr");
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(700);
  });

  it("Enter adds the first product found; the arrows move through the list, round its ends", async () => {
    mockPos({ products_search: () => productHitsFixture });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, {
      route: "/pos",
    });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.type(field, "ca");
    const list = await screen.findByRole("listbox", {
      name: "Produits trouvés",
    });
    const options = within(list).getAllByRole("option");
    expect(options.map((o) => o.getAttribute("aria-selected"))).toEqual(["true", "false"]);
    expect(field).toHaveAttribute("aria-expanded", "true");
    expect(field).toHaveAttribute("aria-activedescendant", options[0]?.id);
    await user.keyboard("{Enter}");
    expect(cartNames()).toEqual([["Lait Candia 1 L", 1_000]]);
    expect(field).toHaveValue("");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(field).toHaveFocus();

    await user.type(field, "ca");
    await screen.findByRole("listbox");
    const selected = () =>
      within(screen.getByRole("listbox"))
        .getAllByRole("option")
        .map((o) => o.getAttribute("aria-selected"));
    await user.keyboard("{ArrowDown}");
    expect(selected()).toEqual(["false", "true"]);
    await user.keyboard("{ArrowDown}");
    expect(selected()).toEqual(["true", "false"]);
    await user.keyboard("{ArrowUp}");
    expect(selected()).toEqual(["false", "true"]);
    await user.keyboard("{Enter}");
    expect(cartNames()).toEqual([
      ["Lait Candia 1 L", 1_000],
      ["Huile Elio 5 L", 1_000],
    ]);
  });

  it("never shows the last search's results for the next text typed", async () => {
    const lait = deferred<ProductHit[]>();
    mockPos({
      products_search: (args) =>
        (args.query as { text: string }).text.toLowerCase().startsWith("la")
          ? lait.promise
          : searchByName(args),
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, {
      route: "/pos",
    });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.type(field, "hu");
    expect(await screen.findByRole("option", { name: /Huile Elio 5 L/ })).toBeInTheDocument();
    await user.keyboard("{Enter}");
    expect(cartNames()).toEqual([["Huile Elio 5 L", 1_000]]);
    // the next product: the oil found a moment ago must not come back under "la"
    await user.type(field, "la");
    expect(screen.queryByRole("listbox")).toBeNull();
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(screen.queryByRole("listbox")).toBeNull();
    await act(async () => lait.resolve([milkHit]));
    const list = await screen.findByRole("listbox");
    expect(within(list).getAllByRole("option")).toHaveLength(1);
    expect(within(list).getByRole("option")).toHaveTextContent("Lait Candia 1 L");
  });

  it("Enter pressed before the results are in waits for them and adds the first product", async () => {
    const huile = deferred<ProductHit[]>();
    const calls = mockPos({
      products_search: (args) =>
        (args.query as { text: string }).text === "huile" ? huile.promise : searchByName(args),
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, {
      route: "/pos",
    });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.type(field, "huile{Enter}");
    expect(useCart.getState().lines).toHaveLength(0);
    await act(async () => huile.resolve([oilHit, milkHit]));
    await waitFor(() => expect(cartNames()).toEqual([["Huile Elio 5 L", 1_000]]));
    expect(field).toHaveValue("");
    expect(calls).not.toContain("products_find_by_code");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("a name nothing matches stays in the field, said quietly; a word with digits is read as a code", async () => {
    const codes: unknown[] = [];
    mockPos({
      products_search: () => [],
      products_find_by_code: ({ code }) => {
        codes.push(code);
        return null;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, {
      route: "/pos",
    });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.type(field, "huile olive");
    expect(await screen.findByText("Aucun produit ne correspond à « huile olive ».")).toBeInTheDocument();
    await user.keyboard("{Enter}");
    expect(field).toHaveValue("huile olive");
    expect(codes).toEqual([]);
    expect(screen.queryByRole("alert")).toBeNull();

    await user.clear(field);
    await user.type(field, "BA2026-000001");
    expect(
      await screen.findByText(
        "Aucun produit ne correspond à « BA2026-000001 ». Appuyez sur Entrée pour le lire comme un code.",
      ),
    ).toBeInTheDocument();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(codes).toEqual(["BA2026-000001"]));
  });

  it("Esc drops the text and its list without opening the menu; a second Esc opens it", async () => {
    mockPos({ products_search: () => productHitsFixture });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, {
      route: "/pos",
    });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.type(field, "ca");
    await screen.findByRole("listbox");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(field).toHaveValue("");
    expect(screen.queryByRole("dialog", { name: "Menu" })).toBeNull();
    await user.keyboard("{Escape}");
    expect(await screen.findByRole("dialog", { name: "Menu" })).toBeInTheDocument();
  });

  it("the pointer moves the highlight, a click adds the product and the field keeps the focus", async () => {
    mockPos({ products_search: () => productHitsFixture });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, {
      route: "/pos",
    });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.type(field, "ca");
    const oil = await screen.findByRole("option", { name: /Huile Elio 5 L/ });
    fireEvent.pointerMove(oil);
    expect(oil).toHaveAttribute("aria-selected", "true");
    await user.click(oil);
    expect(cartNames()).toEqual([["Huile Elio 5 L", 1_000]]);
    expect(field).toHaveValue("");
    expect(field).toHaveFocus();
  });

  it("a tap on a tile clears the text typed and closes the list", async () => {
    mockPos({ products_search: () => productHitsFixture });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, {
      route: "/pos",
    });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.type(field, "ca");
    await screen.findByRole("listbox");
    await user.click(screen.getByRole("button", { name: /Huile Elio 5 L/ }));
    expect(cartNames()).toEqual([["Huile Elio 5 L", 1_000]]);
    expect(field).toHaveValue("");
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("a product found by name takes its unit's decimals, as a scanned one does", async () => {
    const olives = {
      ...oilHit,
      id: "p-olives",
      nameFr: "Olives vertes",
      unitCode: "ctn",
    };
    const calls = mockPos({
      products_search: () => [olives],
      catalog_units_list: () => [
        {
          id: "u-ctn",
          code: "ctn",
          nameAr: "كرتون",
          nameFr: "Carton",
          nameEn: "Carton",
          decimals: 0,
          archived: false,
        },
      ],
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, {
      route: "/pos",
    });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    await waitFor(() => expect(calls).toContain("catalog_units_list"));
    await user.type(field, "ol");
    await screen.findByRole("listbox");
    await user.keyboard("{Enter}");
    expect(useCart.getState().lines[0]?.display.unitDecimals).toBe(0);
  });
});

describe("SaleScreen: removing a line", () => {
  beforeEach(() => {
    useCart.getState().clear("fr");
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(700);
  });

  const PRICES: Record<string, number> = { "p-milk": 15_000, "p-oil": 65_000 };

  /** The echo preview with each line's own total, so a line showing another's figures shows. */
  function pricedEcho(args: Record<string, unknown>) {
    const base = echoPreview(args);
    return {
      ...base,
      cart: {
        ...base.cart,
        lines: base.cart.lines.map((l) => ({
          ...l,
          amounts: { ...l.amounts, totalTtc: PRICES[l.productId ?? ""] ?? 0 },
        })),
      },
    };
  }

  it("right after a void, the line left shows its own name and total while the cart is repriced", async () => {
    let slow: Promise<unknown> | null = null;
    let release = () => {};
    mockPos({
      sales_record_event: () => null,
      sales_preview: (args) => (slow ? slow.then(() => pricedEcho(args)) : pricedEcho(args)),
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={voider} />, { route: "/pos" });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    scan(field, "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    await user.click(await screen.findByRole("button", { name: /Huile Elio 5 L/ }));
    const cart = screen.getByRole("list", { name: "Ticket" });
    await waitFor(() => expect(within(cart).getAllByRole("listitem")[1]).toHaveTextContent("650,00"));
    // the next preview takes its time: the previous cart's figures stay on screen meanwhile
    slow = new Promise((resolve) => {
      release = () => resolve(null);
    });
    field.focus();
    await user.keyboard("{ArrowUp}{Delete}");
    await waitFor(() => expect(within(cart).getAllByRole("listitem")).toHaveLength(1));
    const [left] = within(cart).getAllByRole("listitem");
    expect(left).toHaveTextContent("1 × Huile Elio 5 L");
    expect(left).toHaveTextContent("650,00");
    expect(within(cart).queryByText(/Lait Candia/)).toBeNull();
    expect(screen.getByRole("status", { name: "À payer" })).toHaveAttribute("aria-busy", "true");
    await act(async () => release());
    await waitFor(() =>
      expect(screen.getByRole("status", { name: "À payer" })).not.toHaveAttribute("aria-busy"),
    );
    // the oil line is the selected one: Delete, + and - go on with it
    expect(useCart.getState().selected).toBe(0);
  });

  it("'Supprimer la ligne' keeps the dialog open when the manager's approval is cancelled", async () => {
    let approved = false;
    mockPos({
      sales_record_event: () =>
        approved
          ? null
          : Promise.reject({
              code: "insufficient_permission",
              details: { permission: "sales.void_line" },
            }),
      auth_manager_override: () => {
        approved = true;
        return {
          approverId: "u-owner",
          approverName: "Amine Benali",
          permission: "sales.void_line",
          grantedAt: 1,
          discountLimitBp: 10_000,
        };
      },
      promotions_presets: () => [],
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, {
      route: "/pos",
    });
    const field = await screen.findByLabelText("Scanner ou rechercher un produit");
    scan(field, "6130000000017");
    await user.click(await screen.findByText("1 × Lait Candia 1 L"));
    const edit = await screen.findByRole("dialog", {
      name: "Modifier la ligne",
    });
    await user.type(within(edit).getByLabelText(/Note/), "sans sucre");
    await user.click(within(edit).getByRole("button", { name: "Supprimer la ligne" }));
    const approval = await screen.findByRole("dialog", {
      name: "Accord d'un responsable",
    });
    await user.click(within(approval).getByRole("button", { name: "Annuler" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Accord d'un responsable" })).toBeNull());
    expect(screen.getByRole("dialog", { name: "Modifier la ligne" })).toBeInTheDocument();
    expect(within(edit).getByLabelText(/Note/)).toHaveValue("sans sucre");
    expect(useCart.getState().lines).toHaveLength(1);
    // approved this time: the line goes, and the dialog with it
    await user.click(within(edit).getByRole("button", { name: "Supprimer la ligne" }));
    await user.click(await screen.findByRole("button", { name: /Amine Benali/ }));
    await user.keyboard("2580");
    await user.click(screen.getByRole("button", { name: "Autoriser" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Modifier la ligne" })).toBeNull());
    expect(useCart.getState().lines).toHaveLength(0);
  });
});
