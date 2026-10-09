import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { Document, ReturnExtras, ReturnInput, Serial } from "@/lib/ipc/types";
import { documentFixture, previewFixture, scanHitFixture } from "@/test/fixtures-m1";
import { cashierTill, keyMap, packState } from "@/test/fixtures-m8";
import { type Handler, mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { useCart } from "./cart-store";
import { useExtras } from "./extras-store";
import { SaleScreen } from "./SaleScreen";

afterEach(() => {
  useCart.getState().clear("fr");
  useCart.getState().startExchange(null);
  useExtras.getState().clear();
});

const returner = {
  ...cashierTill().auth_session(),
  permissions: [...cashierTill().auth_session().permissions, "sales.return" as const],
};

function till(modules: string[], extra: Record<string, Handler>) {
  return mockCommands({
    ...cashierTill(),
    packs_state: () => packState({ modules: modules as never }),
    till_key_map: () => keyMap(),
    payment_types_list: () => [],
    ...extra,
  });
}

async function openTicket(user: ReturnType<typeof userEvent.setup>) {
  await screen.findByLabelText("Scanner ou rechercher un produit");
  await user.keyboard("{F8}");
  const dialog = await screen.findByRole("dialog", { name: "Retour de marchandise" });
  await user.type(
    within(dialog).getByLabelText("N° du ticket (scannez son code-barres)"),
    "TK2026-C1-000123{Enter}",
  );
  return dialog;
}

/** The return's serial picker of the phone line (`packs:till.serial.back`). */
const PICKER = "Numéros rendus — Samsung A54";

/** A ticket that sold `count` phones on one line, with their IMEIs. */
function phoneTicket(count: number): { doc: Document; sold: Serial[] } {
  const line = {
    ...documentFixture.lines[0]!,
    id: "l-1",
    productId: "p-a54",
    description: "Samsung A54",
    qty: count * 1_000,
  };
  const imeis = ["356938035643809", "490154203237518", "353918050327655"].slice(0, count);
  return {
    doc: { ...documentFixture, lines: [line] },
    sold: imeis.map((serialNo, i) => ({
      id: `sn-${i + 1}`,
      productId: "p-a54",
      serialNo,
      status: "sold" as const,
      locationId: null,
      receivedDocumentId: null,
      saleLineId: "l-1",
      updatedAt: 0,
    })),
  };
}

/** `returns_quote` as core's `bring_back` decides (sales/serials.rs): a line whose serials were
 * sold names one per piece coming back, unless the whole line comes back without any named. */
function quoting(soldCount: number, quotes: unknown[]): Handler {
  return (args) => {
    quotes.push(args);
    const input = args.input as ReturnInput;
    const extras = args.extras as ReturnExtras | null;
    const named = (extras?.serials ?? []).filter((s) => s.line === 0).length;
    const pieces = Math.floor((input.lines[0]?.qty ?? 0) / 1_000);
    const whole = named === 0 && pieces === soldCount;
    if (!whole && named !== pieces) {
      return Promise.reject({ code: "serials_required", details: { lines: [0] } });
    }
    return { value: pieces * 6_000_000, lines: [] };
  };
}

describe("returns at the till, packs", () => {
  it("an exchange started from an exchange-voucher refund keeps the voucher for the rest", async () => {
    const exchanges: unknown[] = [];
    till(["gift_vouchers"], {
      documents_find: () => documentFixture,
      returns_returnable: () => [{ line: documentFixture.lines[0], returned: 0, remaining: 2_000 }],
      returns_quote: () => ({ value: 15_000, lines: [] }),
      products_find_by_code: () => scanHitFixture,
      returns_exchange: (args) => {
        exchanges.push(args);
        return {
          exchange: {
            returned: {
              document: { ...documentFixture, id: "d-br", docType: "return_note", totalDue: -15_000 },
              refund: 0,
              storeCredit: 15_000,
              exchangeVoucher: null,
            },
            sale: {
              document: documentFixture,
              settlement: previewFixture(30_000, 15_500).settlement,
              shortages: [],
            },
            netToPay: 15_500,
          },
          printQueued: false,
        };
      },
    });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={returner} />);
    const dialog = await openTicket(user);
    await user.type(await within(dialog).findByLabelText("Quantité rendue (Lait Candia 1 L)"), "1");
    await user.click(await within(dialog).findByRole("radio", { name: "Bon d'échange" }));
    await user.click(within(dialog).getByRole("button", { name: "Continuer" }));
    await user.click(
      await within(dialog).findByRole("button", { name: "Échanger contre d'autres articles" }),
    );
    expect(await screen.findByRole("status", { name: "Échange en cours" })).toBeInTheDocument();
    await user.type(await screen.findByRole("combobox", { name: /Scanner/ }), "6130000000017{Enter}");
    await screen.findByText("1 × Lait Candia 1 L");
    await user.keyboard("{F12}");
    const pay = await screen.findByRole("dialog", { name: "Encaissement" });
    await user.click(within(pay).getByRole("button", { name: "Montant exact" }));
    await waitFor(() => expect(within(pay).getByRole("button", { name: /Valider/ })).toBeEnabled());
    await user.click(within(pay).getByRole("button", { name: /Valider/ }));
    await waitFor(() => expect(exchanges).toHaveLength(1));
    // without a customer, a voucher refund is only possible as an exchange voucher (core returns.rs)
    expect(exchanges[0]).toMatchObject({
      request: { ret: { refundMethod: "voucher" }, returnExtras: { exchangeVoucher: true } },
    });
  });

  it("sends no stale serial when the quantity becomes the whole line", async () => {
    const quotes: unknown[] = [];
    const { doc, sold } = phoneTicket(2);
    till(["serials"], {
      documents_find: () => doc,
      returns_returnable: () => [{ line: doc.lines[0], returned: 0, remaining: 2_000 }],
      serials_list: () => sold,
      returns_quote: quoting(2, quotes),
    });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={returner} />);
    const dialog = await openTicket(user);
    const qty = await within(dialog).findByLabelText("Quantité rendue (Samsung A54)");
    await user.type(qty, "1");
    await user.click(await within(dialog).findByRole("checkbox", { name: "356938035643809" }));
    await user.clear(qty);
    await user.type(qty, "2");
    // the whole line comes back: no picker, its serials come back by themselves
    await waitFor(() =>
      expect(within(dialog).queryByRole("group", { name: PICKER })).not.toBeInTheDocument(),
    );
    await user.click(within(dialog).getByRole("button", { name: "Continuer" }));
    await waitFor(() => expect(quotes).toHaveLength(1));
    expect((quotes[0] as { extras: ReturnExtras | null }).extras?.serials ?? []).toEqual([]);
    expect(await within(dialog).findByRole("status", { name: "À rembourser" })).toBeInTheDocument();
    expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();
  });

  it("asks the serials again when the quantity drops below those ticked", async () => {
    const quotes: unknown[] = [];
    const { doc, sold } = phoneTicket(3);
    till(["serials"], {
      documents_find: () => doc,
      returns_returnable: () => [{ line: doc.lines[0], returned: 0, remaining: 3_000 }],
      serials_list: () => sold,
      returns_quote: quoting(3, quotes),
    });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={returner} />);
    const dialog = await openTicket(user);
    const qty = await within(dialog).findByLabelText("Quantité rendue (Samsung A54)");
    await user.type(qty, "2");
    await user.click(await within(dialog).findByRole("checkbox", { name: "356938035643809" }));
    await user.click(within(dialog).getByRole("checkbox", { name: "490154203237518" }));
    await user.clear(qty);
    await user.type(qty, "1");
    // one phone comes back now: the cashier says which one again
    const picker = await within(dialog).findByRole("group", { name: PICKER });
    const boxes = within(picker).getAllByRole("checkbox");
    expect(boxes).toHaveLength(3);
    for (const box of boxes) expect(box).not.toBeChecked();
    await user.click(within(picker).getByRole("checkbox", { name: "490154203237518" }));
    await user.click(within(dialog).getByRole("button", { name: "Continuer" }));
    await waitFor(() => expect(quotes).toHaveLength(1));
    expect((quotes[0] as { extras: ReturnExtras | null }).extras?.serials).toEqual([
      { line: 0, serialNo: "490154203237518" },
    ]);
    expect(await within(dialog).findByRole("status", { name: "À rembourser" })).toBeInTheDocument();
  });
});
