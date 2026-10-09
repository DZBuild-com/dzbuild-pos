import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { documentFixture } from "@/test/fixtures-m1";
import { tillCart, cashierTill, customerIn, packState, paymentTypes, voucherInfo } from "@/test/fixtures-m8";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { useCart } from "./cart-store";
import { useExtras } from "./extras-store";
import { PaymentDialog } from "./PaymentDialog";
import { SaleScreen } from "./SaleScreen";

afterEach(() => {
  useCart.getState().clear("fr");
  useExtras.getState().clear();
});

describe("payment dialog, packs", () => {
  it("spends a gift voucher up to its balance and sends the custom payment type", async () => {
    const sent: unknown[] = [];
    mockCommands({
      ...cashierTill(),
      packs_state: () => packState({ modules: ["gift_vouchers", "loyalty"] }),
      payment_types_list: () => paymentTypes(),
      vouchers_describe: () => voucherInfo({ kind: "gift_voucher", code: "BA2026-000001", balance: 200_000 }),
      sales_create: (args) => {
        sent.push(args);
        return { sale: { document: { id: "d1" } }, printQueued: true };
      },
    });
    tillCart(6_000_000);
    const user = userEvent.setup();
    await renderM8(<PaymentDialog onClose={() => {}} onDone={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "Bon d'achat" }));
    await user.type(await screen.findByLabelText("Code du bon"), "2982600000018{Enter}");
    expect(await screen.findByText("Bon cadeau BA2026-000001 — solde 2 000,00 DA")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Utiliser le bon" }));
    await waitFor(() => expect(screen.getByLabelText("Bon d'achat")).toHaveValue("2000,00"));
    // the backend has counted the voucher: the next tender proposes what is left
    await waitFor(() =>
      expect(screen.getByRole("status", { name: "Reste à payer" })).toHaveTextContent("58 000,00"),
    );
    await user.click(screen.getByRole("button", { name: "Prélèvement CCP" }));
    await user.type(screen.getByLabelText("Référence"), "PRLV 4471");
    await user.click(screen.getByRole("button", { name: "Valider" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({
      request: {
        sale: {
          payments: [
            { method: "voucher", amount: 200_000, reference: "BA2026-000001" },
            { method: "transfer", amount: 5_800_000, reference: "PRLV 4471" },
          ],
        },
        extras: { paymentTypes: [{ payment: 1, paymentTypeId: "00000000-0000-7000-8000-000000000601" }] },
      },
    });
  });

  it("starts the print language at the customer's", async () => {
    mockCommands({
      ...cashierTill(),
      packs_state: () => packState({ modules: [] }),
      payment_types_list: () => [],
    });
    tillCart(100_000, customerIn("ar"));
    await renderM8(<PaymentDialog onClose={() => {}} onDone={() => {}} />);
    const langs = await screen.findByRole("group", { name: "Langue du ticket" });
    await waitFor(() => expect(within(langs).getByRole("radio", { name: "العربية" })).toBeChecked());
  });

  it("shows the points the ticket earns an enrolled customer", async () => {
    mockCommands({
      ...cashierTill(),
      packs_state: () => packState({ modules: ["loyalty"] }),
      payment_types_list: () => [],
      sales_points_preview: () => 23,
    });
    tillCart(2_300_000, customerIn("fr"));
    await renderM8(<PaymentDialog onClose={() => {}} onDone={() => {}} />);
    expect(await screen.findByText("Points gagnés : +23")).toBeInTheDocument();
  });

  it("offers an exchange voucher for a return without a customer", async () => {
    const quotes: unknown[] = [];
    mockCommands({
      ...cashierTill(),
      packs_state: () => packState({ modules: ["gift_vouchers"] }),
      documents_find: () => documentFixture,
      returns_returnable: () => [{ line: documentFixture.lines[0], returned: 0, remaining: 2_000 }],
      returns_quote: (args) => {
        quotes.push(args);
        return { value: 15_000, lines: [] };
      },
    });
    const user = userEvent.setup();
    await renderM8(
      <SaleScreen
        session={{
          ...cashierTill().auth_session(),
          permissions: [...cashierTill().auth_session().permissions, "sales.return"],
        }}
      />,
    );
    await screen.findByLabelText("Scanner ou rechercher un produit");
    await user.keyboard("{F8}");
    const dialog = await screen.findByRole("dialog", { name: "Retour de marchandise" });
    await user.type(
      within(dialog).getByLabelText("N° du ticket (scannez son code-barres)"),
      "TK2026-C1-000123{Enter}",
    );
    await user.type(await within(dialog).findByLabelText("Quantité rendue (Lait Candia 1 L)"), "1");
    await user.click(await within(dialog).findByRole("radio", { name: "Bon d'échange" }));
    await user.click(within(dialog).getByRole("button", { name: "Continuer" }));
    await waitFor(() => expect(quotes).toHaveLength(1));
    expect(quotes[0]).toMatchObject({
      input: { refundMethod: "voucher" },
      extras: { exchangeVoucher: true },
    });
  });
});
