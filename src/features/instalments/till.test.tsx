import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCart } from "@/features/pos/cart-store";
import { SaleScreen } from "@/features/pos/SaleScreen";
import type { Cart } from "@/lib/ipc/types";
import { cashierSession, companyFixture, settingsValues } from "@/test/fixtures";
import {
  cashSessionFixture,
  categoriesFixture,
  previewFixture,
  priceLevelsFixture,
  productHitsFixture,
} from "@/test/fixtures-m1";
import {
  customerWithLang,
  instalmentQuote,
  packState,
  phoneExtras,
  plan,
  serialsInStock,
} from "@/test/fixtures-m8";
import { useExtras } from "@/features/pos/extras-store";
import { mockCommands, renderWithProviders } from "@/test/render";

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

/** A fridge for Amina, rung up at the till. */
const fridgeCart: Cart = {
  lines: [
    {
      productId: "p-fridge",
      description: null,
      qty: 1_000,
      unitPrice: null,
      discount: { kind: "none" },
      taxRateId: null,
      note: null,
      expectedUnitPrice: 12_000_000,
    },
  ],
  priceLevelId: null,
  customerId: "c-amina",
  lang: "fr",
  note: null,
};

/** The till with Amina's fridge rung up; `signed` collects the `instalments_sign` calls. With
 * `serials`, the serials module is on and the fridge is sold by its serial number. */
function mockTill(signed: unknown[], serials = false) {
  mockCommands({
    ...(serials
      ? {
          products_extras_get: () => ({ ...phoneExtras(), productId: "p-fridge" }),
          serials_available: () => serialsInStock().map((s) => ({ ...s, productId: "p-fridge" })),
        }
      : {}),
    auth_session: () => cashierSession,
    company_get: () => ({ ...companyFixture, defaultDocLang: "fr" }),
    settings_get: ({ key }) => settingsValues[String(key)] ?? null,
    hardware_get: () => hardware,
    cash_current: () => cashSessionFixture,
    catalog_categories_list: () => categoriesFixture,
    catalog_price_levels_list: () => priceLevelsFixture,
    products_grid: () => productHitsFixture,
    sales_held: () => [],
    sales_preview: () => previewFixture(12_000_000, 0, 0),
    packs_state: () => packState({ modules: serials ? ["instalments", "serials"] : ["instalments"] }),
    customers_get: () => customerWithLang("ar"),
    instalments_quote: () => instalmentQuote(),
    instalments_sign: (args) => {
      signed.push(args);
      return {
        signed: { plan: plan(), sale: { document: { id: "d1", number: "TK2026-000050" } } },
        printQueued: false,
      };
    },
  });
  useCart
    .getState()
    .load(fridgeCart, [{ name: "Réfrigérateur", unitCode: "pc", unitDecimals: 0, price: 12_000_000 }]);
}

/** The menu's "Vente à tempérament" over the till: the wizard's dialog, its customer loaded. */
async function openWizard(user: ReturnType<typeof userEvent.setup>) {
  renderWithProviders(<SaleScreen session={cashierSession} />, { route: "/pos" });
  await screen.findByText("1 × Réfrigérateur");
  await user.keyboard("{Escape}");
  const menu = await screen.findByRole("dialog", { name: "Menu" });
  await user.click(await within(menu).findByRole("button", { name: "Vente à tempérament" }));
  const wizard = await screen.findByRole("dialog", { name: "Vente à tempérament" });
  // the till's customer is the contract's: the first step can be passed at once
  expect(await within(wizard).findByText("Amina Belkacem")).toBeInTheDocument();
  return wizard;
}

describe("an instalment sale from the till", () => {
  beforeEach(() => {
    useCart.getState().clear("fr");
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(700);
  });

  it("starts from the till's cart and customer, and the till is empty once the contract is signed", async () => {
    const signed: unknown[] = [];
    mockTill(signed);
    const user = userEvent.setup();
    const wizard = await openWizard(user);
    await user.click(within(wizard).getByRole("button", { name: "Suivant" }));
    await user.click(within(wizard).getByRole("button", { name: "Suivant" }));
    await within(wizard).findByRole("table", { name: "Échéancier" });
    await user.click(within(wizard).getByRole("button", { name: "Suivant" }));
    await user.click(within(wizard).getByRole("button", { name: "Signer et encaisser l'apport" }));
    await waitFor(() =>
      expect(signed[0]).toMatchObject({
        input: { quote: { cart: { customerId: "c-amina", lines: [{ productId: "p-fridge", qty: 1_000 }] } } },
      }),
    );
    expect(await within(wizard).findByText("Contrat CT2026-000001 signé")).toBeInTheDocument();
    expect(useCart.getState().lines).toEqual([]);
  });

  it("asks the serial of a serial-tracked good at the signature and sends it (final review)", async () => {
    const signed: unknown[] = [];
    mockTill(signed, true);
    const user = userEvent.setup();
    const wizard = await openWizard(user);
    await user.click(within(wizard).getByRole("button", { name: "Suivant" }));
    await user.click(within(wizard).getByRole("button", { name: "Suivant" }));
    await within(wizard).findByRole("table", { name: "Échéancier" });
    await user.click(within(wizard).getByRole("button", { name: "Suivant" }));
    await user.click(within(wizard).getByRole("button", { name: "Signer et encaisser l'apport" }));
    const dialog = await screen.findByRole("dialog", { name: /Numéro de série/ });
    await user.click(await within(dialog).findByRole("button", { name: "356938035643809" }));
    await user.click(within(dialog).getByRole("button", { name: "Valider" }));
    await waitFor(() => expect(signed).toHaveLength(1));
    expect(signed[0]).toMatchObject({
      input: { extras: { serials: [{ line: 0, serialNo: "356938035643809" }] } },
    });
  });

  it("takes the serial the till already has for the line, without asking again", async () => {
    const signed: unknown[] = [];
    mockTill(signed, true);
    useExtras.getState().setSerials(0, ["490154203237518"]);
    const user = userEvent.setup();
    const wizard = await openWizard(user);
    await user.click(within(wizard).getByRole("button", { name: "Suivant" }));
    await user.click(within(wizard).getByRole("button", { name: "Suivant" }));
    await within(wizard).findByRole("table", { name: "Échéancier" });
    await user.click(within(wizard).getByRole("button", { name: "Suivant" }));
    await user.click(within(wizard).getByRole("button", { name: "Signer et encaisser l'apport" }));
    await waitFor(() => expect(signed).toHaveLength(1));
    expect(signed[0]).toMatchObject({
      input: { extras: { serials: [{ line: 0, serialNo: "490154203237518" }] } },
    });
    expect(screen.queryByRole("dialog", { name: /Numéro de série/ })).toBeNull();
    useExtras.getState().clear();
  });

  it("goes back one step on Esc, keeping what was typed, and Esc on the first step closes it", async () => {
    mockTill([]);
    const user = userEvent.setup();
    const wizard = await openWizard(user);
    const next = () => user.click(within(wizard).getByRole("button", { name: "Suivant" }));
    await next();
    await next();
    await within(wizard).findByRole("table", { name: "Échéancier" });
    await next();
    // the supporting documents: a guarantor typed, the focus still in his field
    await user.type(within(wizard).getByLabelText("Titulaire"), "Karim Belkacem");
    await user.keyboard("{Escape}");
    // one step back (the dialog's Esc does not close the wizard, nor go back twice): the terms
    expect(screen.getByRole("dialog", { name: "Vente à tempérament" })).toBe(wizard);
    expect(within(wizard).getByText("Étape 3 sur 5")).toBeInTheDocument();
    expect(within(wizard).getByRole("table", { name: "Échéancier" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog", { name: "Vente à tempérament" })).toBe(wizard);
    expect(within(wizard).getByText("Étape 2 sur 5")).toBeInTheDocument();
    // forward again: the guarantor is still there
    await next();
    await waitFor(() => expect(within(wizard).getByRole("button", { name: "Suivant" })).toBeEnabled());
    await next();
    expect(within(wizard).getByLabelText("Titulaire")).toHaveValue("Karim Belkacem");
    // back to the customer, then Esc closes the wizard: nothing signed, the till keeps its sale
    await user.keyboard("{Escape}{Escape}{Escape}");
    expect(within(wizard).getByText("Étape 1 sur 5")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Vente à tempérament" })).not.toBeInTheDocument(),
    );
    expect(useCart.getState().lines).toHaveLength(1);
  });
});
