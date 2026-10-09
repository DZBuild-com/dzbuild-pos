import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useCart } from "@/features/pos/cart-store";
import { useExtras } from "@/features/pos/extras-store";
import { SaleScreen } from "@/features/pos/SaleScreen";
import { TillKitContext } from "@/features/pos/till-kit";
import { tillKit } from "@/features/pos/till-kit-parts";
import type { Cart, Module } from "@/lib/ipc/types";
import { cashierSession } from "@/test/fixtures";
import { previewFixture, saleDoneFixture } from "@/test/fixtures-m1";
import { cashierTill, keyMap, order, packState, phoneExtras, serialsInStock } from "@/test/fixtures-m8";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";

vi.mock("@/lib/sounds", () => ({ beep: () => undefined }));

/** Amina's order of one Samsung A54 (or a medicine), 60 000,00 DA, nothing held. */
const phoneOrder = () => ({
  ...order(),
  lines: [
    { productId: "p-a54", description: "Samsung A54", qty: 1_000, unitPrice: 6_000_000, totalTtc: 6_000_000 },
  ],
});

const phoneCart: Cart = {
  lines: [
    {
      productId: "p-a54",
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

function preview({ input }: Record<string, unknown>) {
  const { tenders } = input as { tenders: { amount: number }[] };
  const base = previewFixture(6_000_000, 0, 0);
  const paid = tenders.reduce((sum, t) => sum + t.amount, 0);
  return {
    ...base,
    settlement: { ...base.settlement, paidTotal: paid, missing: Math.max(0, 6_000_000 - paid), change: 0 },
  };
}

function mockFulfil(modules: Module[], sent: unknown[], extra: Record<string, Handler>) {
  return mockCommands({
    ...cashierTill(),
    packs_state: () => packState({ modules }),
    till_key_map: () => keyMap(),
    sales_preview: preview,
    orders_get: () => phoneOrder(),
    orders_fulfil_cart: () => phoneCart,
    orders_fulfil: (args) => {
      sent.push(args);
      return {
        fulfilled: { order: order({ status: "fulfilled" }), sale: saleDoneFixture.sale },
        printQueued: false,
      };
    },
    ...extra,
  });
}

/** The till with the order served from `/customer-orders` ("Servir en caisse"). */
function serve() {
  return renderWithProviders(
    <TillKitContext.Provider value={tillKit}>
      <SaleScreen session={cashierSession} />
    </TillKitContext.Provider>,
    { route: "/pos", state: { fulfil: "o1" } },
  );
}

async function payCash(user: ReturnType<typeof userEvent.setup>) {
  const pay = await screen.findByRole("dialog", { name: "Encaissement" });
  await user.type(within(pay).getByLabelText("Espèces"), "60000");
  const validate = within(pay).getByRole("button", { name: "Valider" });
  await waitFor(() => expect(validate).toBeEnabled());
  await user.click(validate);
}

describe("an order served at the till asks what its lines need (final review)", () => {
  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(700);
  });
  afterEach(() => {
    useCart.getState().clear("fr");
    useExtras.getState().clear();
  });

  it("asks the IMEI of a serial-tracked phone and sends it with the sale", async () => {
    const sent: unknown[] = [];
    mockFulfil(["orders", "serials"], sent, {
      products_extras_get: () => phoneExtras(),
      serials_available: () => serialsInStock(),
    });
    const user = userEvent.setup();
    serve();
    const dialog = await screen.findByRole("dialog", { name: /Numéro de série/ });
    await user.click(await within(dialog).findByRole("button", { name: "356938035643809" }));
    await user.click(within(dialog).getByRole("button", { name: "Valider" }));
    await payCash(user);
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({
      input: { id: "o1", extras: { serials: [{ line: 0, serialNo: "356938035643809" }] } },
    });
  });

  it("asks the register entry of a controlled medicine and sends it with the sale", async () => {
    const sent: unknown[] = [];
    mockFulfil(["orders", "pharmacy"], sent, {
      pharmacy_product_get: ({ productId }) => ({
        productId,
        dci: "TRAMADOL",
        controlled: true,
        schedule: "II",
        prescriptionRequired: true,
      }),
      pharmacy_equivalents: () => [],
      chifa_of_product: () => null,
    });
    const user = userEvent.setup();
    serve();
    const form = await screen.findByRole("dialog", { name: "Registre des médicaments contrôlés" });
    await user.type(within(form).getByLabelText("Prescripteur"), "Dr Meziane");
    await user.type(within(form).getByLabelText("Date de l'ordonnance"), "2026-10-02");
    await user.type(within(form).getByLabelText("Patient"), "M. Haddad");
    await user.click(within(form).getByRole("button", { name: "Enregistrer et continuer" }));
    await payCash(user);
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({
      input: {
        extras: { prescriptions: [{ line: 0, prescriberName: "Dr Meziane", patientName: "M. Haddad" }] },
      },
    });
  });
});
