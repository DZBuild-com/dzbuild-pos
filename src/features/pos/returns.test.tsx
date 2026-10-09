import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useCart } from "@/features/pos/cart-store";
import { SaleScreen } from "@/features/pos/SaleScreen";
import { setLanguage } from "@/lib/i18n";
import { setBurstGuard } from "@/lib/scanner";
import { cashierSession, companyFixture, ownerTile, settingsValues } from "@/test/fixtures";
import {
  cashSessionFixture,
  categoriesFixture,
  documentFixture,
  previewFixture,
  priceLevelsFixture,
  productHitsFixture,
  scanHitFixture,
  taxRatesFixture,
} from "@/test/fixtures-m1";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";
import { scanDigits, scannerKeys } from "@/test/scan";

const returnable = [{ line: documentFixture.lines[0], returned: 0, remaining: 2_000 }];
const returnDone = {
  returned: {
    document: {
      ...documentFixture,
      id: "d-br",
      docType: "return_note",
      number: "BR2026-C1-000004",
      totalDue: -15_000,
    },
    refund: 15_000,
    storeCredit: 0,
  },
  printQueued: false,
};

function mockReturns(extra: Record<string, Handler> = {}) {
  return mockCommands({
    auth_session: () => cashierSession,
    auth_list_users: () => [ownerTile],
    company_get: () => ({ ...companyFixture, defaultDocLang: "fr" }),
    settings_get: ({ key }) => settingsValues[String(key)] ?? null,
    hardware_get: () => ({
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
    }),
    cash_current: () => cashSessionFixture,
    catalog_categories_list: () => categoriesFixture,
    catalog_price_levels_list: () => priceLevelsFixture,
    catalog_tax_rates_list: () => taxRatesFixture,
    products_grid: () => productHitsFixture,
    products_search: () => productHitsFixture,
    products_find_by_code: () => scanHitFixture,
    sales_held: () => [],
    sales_preview: () => previewFixture(30_000, 0),
    documents_find: ({ number }) => (number === "TK2026-C1-000123" ? documentFixture : null),
    returns_returnable: () => returnable,
    auth_manager_override: () => ({
      approverId: "u-owner",
      approverName: "Amine Benali",
      permission: "sales.return",
      grantedAt: 1,
    }),
    ...extra,
  });
}

async function approve(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: /Amine Benali/ }));
  await user.keyboard("2580");
  await user.click(screen.getByRole("button", { name: "Autoriser" }));
}

async function findTicket(user: ReturnType<typeof userEvent.setup>) {
  await screen.findByLabelText("Scanner ou rechercher un produit");
  await user.keyboard("{F8}");
  const dialog = await screen.findByRole("dialog", { name: "Retour de marchandise" });
  await user.type(
    within(dialog).getByLabelText("N° du ticket (scannez son code-barres)"),
    "tk2026-c1-000123{Enter}",
  );
  expect(await within(dialog).findByText("Lait Candia 1 L")).toBeInTheDocument();
  return dialog;
}

describe("returns at the till", () => {
  beforeEach(() => useCart.getState().clear("fr"));

  it("refunds part of a ticket after a manager approves", async () => {
    let quotes = 0;
    const created: unknown[] = [];
    mockReturns({
      returns_quote: () =>
        ++quotes === 1
          ? Promise.reject({ code: "insufficient_permission", details: { permission: "sales.return" } })
          : { value: 15_000, lines: [] },
      returns_create: (args) => {
        created.push(args);
        return returnDone;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    const dialog = await findTicket(user);
    // an anonymous ticket has no account to refund to (RET-007)
    expect(within(dialog).queryByRole("radio", { name: "Crédit client" })).toBeNull();
    await user.type(within(dialog).getByLabelText("Quantité rendue (Lait Candia 1 L)"), "1");
    await user.selectOptions(within(dialog).getByLabelText("Motif (Lait Candia 1 L)"), "changed_mind");
    await user.click(within(dialog).getByRole("button", { name: "Continuer" }));
    await approve(user);
    const confirm = await screen.findByRole("dialog", { name: "Retour de marchandise" });
    expect(within(confirm).getByRole("status", { name: "À rembourser" })).toHaveTextContent("150,00");
    await user.click(within(confirm).getByRole("button", { name: "Rembourser" }));
    expect(await screen.findByRole("dialog", { name: "Retour enregistré" })).toBeInTheDocument();
    expect(screen.getByText("BR2026-C1-000004")).toBeInTheDocument();
    expect(created[0]).toMatchObject({
      request: {
        printReceipt: false,
        input: {
          originalId: "d-1",
          refundMethod: "cash",
          lines: [
            {
              originLineId: "l-1",
              productId: null,
              qty: 1_000,
              unitPrice: null,
              restock: true,
              reason: "changed_mind",
              note: null,
            },
          ],
        },
      },
    });
  });

  it("never returns more than what is left on the ticket", async () => {
    mockReturns({ returns_quote: () => ({ value: 45_000, lines: [] }) });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    const dialog = await findTicket(user);
    await user.type(within(dialog).getByLabelText("Quantité rendue (Lait Candia 1 L)"), "3");
    await user.click(within(dialog).getByRole("button", { name: "Continuer" }));
    expect(await within(dialog).findByText("Au plus 2 pour cette ligne.")).toBeInTheDocument();
  });

  it("an exchange pays the new ticket with the returned value", async () => {
    const previews: unknown[] = [];
    const exchanges: unknown[] = [];
    mockReturns({
      returns_quote: () => ({ value: 15_000, lines: [] }),
      // 305,00 DA due; the returned value and the cash typed pay it
      sales_preview: (args) => {
        previews.push(args);
        const input = args.input as { storeCredit: number | null; tenders: { amount: number }[] };
        const credit = input.storeCredit ?? 0;
        const cash = input.tenders.reduce((sum, p) => sum + p.amount, 0);
        const base = previewFixture(30_000, cash);
        const paid = credit + cash;
        return {
          ...base,
          settlement: {
            ...base.settlement,
            storeCredit: credit,
            nonCash: credit,
            paidTotal: paid,
            missing: Math.max(0, 30_500 - paid),
            change: Math.max(0, paid - 30_500),
          },
        };
      },
      returns_exchange: (args) => {
        exchanges.push(args);
        return {
          exchange: {
            returned: returnDone.returned,
            sale: {
              document: documentFixture,
              settlement: previewFixture(30_000, 20_000).settlement,
              shortages: [],
            },
            netToPay: 15_500,
          },
          printQueued: false,
        };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(
      <SaleScreen
        session={{ ...cashierSession, permissions: [...cashierSession.permissions, "sales.return"] }}
      />,
      { route: "/pos" },
    );
    const dialog = await findTicket(user);
    await user.type(within(dialog).getByLabelText("Quantité rendue (Lait Candia 1 L)"), "1");
    await user.click(within(dialog).getByRole("button", { name: "Continuer" }));
    await user.click(
      await within(dialog).findByRole("button", { name: "Échanger contre d'autres articles" }),
    );
    expect(await screen.findByRole("status", { name: "Échange en cours" })).toHaveTextContent("150,00");
    const field = screen.getByLabelText("Scanner ou rechercher un produit");
    scanDigits(field, "6130000000017");
    await screen.findByText("1 × Lait Candia 1 L");
    await user.keyboard("{F12}");
    const pay = await screen.findByRole("dialog", { name: "Encaissement" });
    await waitFor(() => expect(previews.at(-1)).toMatchObject({ input: { storeCredit: 15_000 } }));
    expect(within(pay).getByText(/Valeur reprise/)).toBeInTheDocument();
    await user.click(within(pay).getByRole("button", { name: "Montant exact" }));
    await waitFor(() => expect(within(pay).getByRole("button", { name: /Valider/ })).toBeEnabled());
    expect(previews.at(-1)).toMatchObject({
      input: { storeCredit: 15_000, tenders: [{ method: "cash", amount: 15_500 }] },
    });
    await user.click(within(pay).getByRole("button", { name: /Valider/ }));
    await waitFor(() => expect(exchanges).toHaveLength(1));
    expect(exchanges[0]).toMatchObject({
      request: { ret: { originalId: "d-1" }, sale: { cart: { lines: [{ productId: "p-milk" }] } } },
    });
    expect(await screen.findByRole("dialog", { name: "Vente encaissée" })).toBeInTheDocument();
  });

  it("takes a return without a receipt by product search", async () => {
    const quotes: unknown[] = [];
    mockReturns({
      returns_quote: (args) => {
        quotes.push(args);
        return { value: 65_000, lines: [] };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(
      <SaleScreen
        session={{
          ...cashierSession,
          permissions: [...cashierSession.permissions, "sales.return", "sales.price_override"],
        }}
      />,
      { route: "/pos" },
    );
    await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.keyboard("{F8}");
    const dialog = await screen.findByRole("dialog", { name: "Retour de marchandise" });
    await user.click(within(dialog).getByRole("button", { name: "Retour sans ticket" }));
    await user.type(within(dialog).getByLabelText("Chercher le produit rendu"), "elio");
    await user.click(await within(dialog).findByRole("button", { name: /Huile Elio 5 L/ }));
    await user.type(within(dialog).getByLabelText("Quantité rendue (Huile Elio 5 L)"), "1");
    await user.click(within(dialog).getByRole("button", { name: "Continuer" }));
    await waitFor(() => expect(quotes).toHaveLength(1));
    expect(quotes[0]).toMatchObject({
      input: {
        originalId: null,
        lines: [{ productId: "p-oil", originLineId: null, qty: 1_000, unitPrice: 65_000 }],
      },
    });
  });

  it("opens on the ticket the sales history sent", async () => {
    mockReturns();
    renderWithProviders(<SaleScreen session={cashierSession} />, {
      route: "/pos",
      state: { returnNumber: "TK2026-C1-000123" },
    });
    const dialog = await screen.findByRole("dialog", { name: "Retour de marchandise" });
    expect(await within(dialog).findByText("Lait Candia 1 L")).toBeInTheDocument();
    expect(within(dialog).getByText("(reste 2 à rendre)")).toBeInTheDocument();
    expect(within(dialog).getByText("TK2026-C1-000123")).toBeInTheDocument();
  });

  it("reads a scanned receipt barcode from the physical keys on an AZERTY Windows", async () => {
    // A scanner emulating a US keyboard, on a French AZERTY Windows: Windows types "è'('" where
    // the scanner sent "2026", and ")" for "-". The field decodes the physical keys instead.
    setBurstGuard(true);
    const found: unknown[] = [];
    mockReturns({
      documents_find: ({ number }) => {
        found.push(number);
        return number === "TK2026-C1-000123" ? documentFixture : null;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.keyboard("{F8}");
    const dialog = await screen.findByRole("dialog", { name: "Retour de marchandise" });
    const field = within(dialog).getByLabelText("N° du ticket (scannez son code-barres)");
    const AZERTY: Record<string, string> = {
      "0": "à",
      "1": "&",
      "2": "é",
      "3": '"',
      "4": "'",
      "5": "(",
      "6": "-",
      "7": "è",
      "8": "_",
      "9": "ç",
    };
    scannerKeys(field, [
      ...[..."TK2026-C1-000123"].map((ch) =>
        ch === "-"
          ? { code: "Minus", key: ")" }
          : /\d/.test(ch)
            ? { code: `Digit${ch}`, key: AZERTY[ch] ?? ch }
            : { code: `Key${ch}`, key: ch, shiftKey: true },
      ),
      { code: "Enter", key: "Enter" },
    ]);
    expect(await within(dialog).findByText("Lait Candia 1 L")).toBeInTheDocument();
    expect(found).toEqual(["TK2026-C1-000123"]);
  });

  it("says when everything on the ticket has already come back", async () => {
    mockReturns({ returns_returnable: () => [{ ...returnable[0], returned: 2_000, remaining: 0 }] });
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
    await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.keyboard("{F8}");
    const dialog = await screen.findByRole("dialog", { name: "Retour de marchandise" });
    await user.type(
      within(dialog).getByLabelText("N° du ticket (scannez son code-barres)"),
      "TK2026-C1-000123{Enter}",
    );
    expect(
      await within(dialog).findByText("Tout ce ticket a déjà été rendu : il ne reste rien à reprendre."),
    ).toBeInTheDocument();
    expect(within(dialog).queryByLabelText("Quantité rendue (Lait Candia 1 L)")).not.toBeInTheDocument();
  });

  it("reads right to left in Arabic", async () => {
    await setLanguage("ar");
    mockReturns();
    const user = userEvent.setup();
    renderWithProviders(<SaleScreen session={{ ...cashierSession, lang: "ar" }} />, { route: "/pos" });
    await screen.findByLabelText("امسح أو ابحث عن منتج");
    await user.keyboard("{F8}");
    expect(await screen.findByRole("dialog", { name: "إرجاع سلع" })).toBeInTheDocument();
    expect(document.documentElement).toHaveAttribute("dir", "rtl");
  });
});
