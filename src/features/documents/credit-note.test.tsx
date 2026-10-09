import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Toaster } from "sonner";
import { describe, expect, it } from "vitest";
import { StatementA4Actions } from "@/features/customers/StatementA4Actions";
import type { DocumentParty, WarrantySource } from "@/lib/ipc/types";
import { mockCommands, renderWithProviders } from "@/test/render";
import { ownerSession } from "@/test/fixtures";
import { previewFixture } from "@/test/fixtures-m1";
import {
  creditableFixture,
  creditQuoteFixture,
  docStateFixture,
  draftPreviewFixture,
  draftQuoteFixture,
  mentionsCompleteFixture,
  validatedInvoiceFixture,
} from "@/test/fixtures-m3";
import { ArabicGuardBanner } from "./ArabicGuardBanner";
import { CreditNoteDialog } from "./CreditNoteDialog";
import { DocumentView } from "./DocumentView";
import { DocumentEditor } from "./editor/DocumentEditor";
import { WarrantyDialog } from "./WarrantyDialog";

const creditNote = {
  ...validatedInvoiceFixture,
  id: "av-1",
  docType: "credit_note" as const,
  number: "AV2026-000003",
  totalTtc: -357_000,
};

/** An amount as `Amount` prints it, with its exact invisible spaces (Testing Library turns them
 * into plain spaces in the text it compares, never in the string it is given). */
const amount = (text: string) => (_: string, element: Element | null) =>
  element?.tagName === "BDI" && element.textContent === text;

describe("credit notes", () => {
  it("credits part of a line with the backend's figures and prints the credit note", async () => {
    const created: unknown[] = [];
    mockCommands({
      documents_creditable: () => creditableFixture,
      documents_credit_quote: () => creditQuoteFixture,
      documents_create_credit_note: (args) => {
        created.push(args);
        return {
          document: creditNote,
          refund: 357_000,
          customerBalance: 1_443_000,
          invoiceFullyCredited: false,
        };
      },
      hardware_get: () => ({
        version: 1,
        printers: [],
        receiptPrinterId: null,
        receipt: {},
        scanner: { layout: "us" },
        documents: { defaultPrinter: null, kinds: [], marginsMm: 12 },
      }),
      hardware_discover: () => [],
    });
    // The app shell's toaster shows the "créé" toast.
    renderWithProviders(
      <>
        <CreditNoteDialog invoice={validatedInvoiceFixture} open onClose={() => {}} />
        <Toaster />
      </>,
    );
    const dialog = await screen.findByRole("dialog", { name: "Avoir sur la facture FA2026-000045" });
    await userEvent.type(within(dialog).getByLabelText("Quantité à reprendre — Ciment 25 kg"), "3");
    await userEvent.selectOptions(within(dialog).getByLabelText("Motif"), "defective");
    await userEvent.selectOptions(within(dialog).getByLabelText("Remboursement"), "credit");
    expect(await within(dialog).findByText(amount("-3\u202F570,00\u00A0DA"))).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Créer l'avoir" }));
    await waitFor(() => expect(created).toHaveLength(1));
    expect(created[0]).toMatchObject({
      input: {
        invoiceId: "inv-1",
        kind: "goods",
        lines: [{ originLineId: "l-cement", qty: 3_000, restock: true }],
        reason: "defective",
        refundMethod: "credit",
      },
    });
    expect(await screen.findByText("Avoir AV2026-000003 créé.")).toBeInTheDocument();
    expect(await screen.findByRole("dialog", { name: "Imprimer AV2026-000003" })).toBeInTheDocument();
  });

  it("never lets a quantity above what remains reach the backend", async () => {
    const quoted: unknown[] = [];
    mockCommands({
      documents_creditable: () => creditableFixture,
      documents_credit_quote: (args) => {
        quoted.push(args);
        return creditQuoteFixture;
      },
    });
    renderWithProviders(<CreditNoteDialog invoice={validatedInvoiceFixture} open onClose={() => {}} />);
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Quantité à reprendre — Ciment 25 kg"), "11");
    expect(within(dialog).getByText("10 au plus")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Créer l'avoir" })).toBeDisabled();
    expect(quoted.every((q) => JSON.stringify(q).includes('"qty":11000') === false)).toBe(true);
  });
});

const seller: DocumentParty = {
  role: "seller",
  partyId: null,
  name: "Boutique Amine",
  nameAlt: null,
  legalForm: null,
  activity: null,
  activityAlt: null,
  capital: null,
  address: null,
  wilaya: null,
  phone: null,
  email: null,
  rc: null,
  aeCard: null,
  nif: null,
  nis: null,
  ai: null,
  nin: null,
  rib: null,
  vatExemptRef: null,
};

const warrantyLine = (lineId: string, product: string, serials: string[]): WarrantySource => ({
  documentId: "inv-1",
  docType: "invoice",
  number: "FA2026-000045",
  businessDate: "2026-09-28",
  seller,
  buyer: null,
  lineId,
  product,
  brand: "Samsung",
  serials,
  unitPriceTtc: 4_500_000,
  qty: 1_000,
  months: 12,
});

describe("document view actions", () => {
  it("offers a credit note and a warranty certificate on a validated invoice", async () => {
    mockCommands({
      documents_get: () => validatedInvoiceFixture,
      documents_state: () => docStateFixture,
      documents_links: () => ({ sources: [], targets: [] }),
      documents_warranty_lines: () => [warrantyLine("l-1", "Galaxy A15", [])],
      documents_creditable: () => creditableFixture,
    });
    renderWithProviders(<DocumentView id="inv-1" session={ownerSession} />);
    expect(await screen.findByRole("button", { name: "Certificat de garantie" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Avoir" }));
    expect(
      await screen.findByRole("dialog", { name: "Avoir sur la facture FA2026-000045" }),
    ).toBeInTheDocument();
  });

  it("offers no credit note on an invoice fully credited", async () => {
    mockCommands({
      documents_get: () => validatedInvoiceFixture,
      documents_state: () => ({ state: "credited", by: ["AV2026-000003"] }),
      documents_links: () => ({ sources: [], targets: [] }),
    });
    renderWithProviders(<DocumentView id="inv-1" session={ownerSession} />);
    // The numbers sit in their own <bdi dir="ltr"> inside the badge: the badge's whole text is read.
    const badge = await screen.findByText((_, element) => element?.getAttribute("data-state") === "credited");
    expect(badge.textContent).toBe("Annulée par avoir AV2026-000003");
    expect(screen.queryByRole("button", { name: "Avoir" })).not.toBeInTheDocument();
  });
});

describe("warranty certificates", () => {
  it("prints the certificate of the chosen line in the chosen language", async () => {
    const printed: unknown[] = [];
    mockCommands({
      documents_warranty_lines: () => [
        warrantyLine("l-1", "Galaxy A15", ["356789012345678"]),
        warrantyLine("l-2", "Galaxy Tab A9", []),
      ],
      documents_get: () => validatedInvoiceFixture,
      warranty_print: (args) => {
        printed.push(args);
        return { printer: null, copies: 1, duplicate: false, dialog: false };
      },
    });
    renderWithProviders(
      <>
        <WarrantyDialog documentId="inv-1" open onClose={() => {}} />
        <Toaster />
      </>,
    );
    const dialog = await screen.findByRole("dialog", { name: "Certificat de garantie" });
    expect(await within(dialog).findByText("356789012345678")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("radio", { name: /Galaxy Tab A9/ }));
    await userEvent.click(within(dialog).getByRole("radio", { name: "العربية" }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Imprimer" }));
    await waitFor(() =>
      expect(printed).toEqual([{ input: { documentId: "inv-1", lineId: "l-2", lang: "ar" } }]),
    );
    expect(await screen.findByText("Certificat de garantie imprimé.")).toBeInTheDocument();
  });
});

describe("A4 statements", () => {
  it("prints a supplier's statement on A4 and saves it as a PDF in the chosen language", async () => {
    const asked: unknown[] = [];
    mockCommands({
      statements_print: (args) => {
        asked.push(["print", args]);
        return { printer: null, copies: 1, duplicate: false, dialog: false };
      },
      statements_export_pdf: (args) => {
        asked.push(["pdf", args]);
        return { path: "D:/releve-F0001.pdf", duplicate: false };
      },
    });
    renderWithProviders(
      <>
        <StatementA4Actions side="supplier" partyId="s-cevital" from="2026-09-01" to="2026-09-30" />
        <Toaster />
      </>,
    );
    await userEvent.click(screen.getByRole("radio", { name: "English" }));
    await userEvent.click(screen.getByRole("button", { name: "Imprimer en A4" }));
    expect(await screen.findByText("Relevé imprimé en A4.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "PDF" }));
    expect(await screen.findByText("PDF enregistré : D:/releve-F0001.pdf")).toBeInTheDocument();
    const input = {
      side: "supplier",
      partyId: "s-cevital",
      from: "2026-09-01",
      to: "2026-09-30",
      lang: "en",
    };
    expect(asked).toEqual([
      ["print", { input }],
      ["pdf", { input }],
    ]);
  });
});

describe("Arabic guard", () => {
  it("suggests Arabic for a consumer document without blocking", async () => {
    mockCommands({ documents_arabic_guard: () => ({ suggested: "ar", categoryIds: [] }) });
    const switched: string[] = [];
    renderWithProviders(
      <ArabicGuardBanner
        input={{ lang: "fr", secondLang: null, productIds: ["p-milk"], consumer: true }}
        onSwitch={(l) => switched.push(l)}
      />,
    );
    await userEvent.click(await screen.findByRole("button", { name: "Imprimer en arabe" }));
    expect(switched).toEqual(["ar"]);
  });

  it("puts an invoice to a private person in Arabic first, its language second, and saves it", async () => {
    const saved: unknown[] = [];
    mockCommands({
      documents_draft_cart: () => ({
        id: "d4",
        docType: "invoice",
        cart: { lines: [], priceLevelId: null, customerId: "c-karim", lang: "fr", note: null },
        validUntil: null,
        dueDate: null,
        secondLang: null,
        origin: null,
      }),
      documents_mentions: () => ({ ...mentionsCompleteFixture, buyerKind: "consumer" }),
      documents_preview_draft: () => draftPreviewFixture,
      customers_get: () => ({ id: "c-karim", name: "Karim" }),
      documents_payment_preview: () => ({ ...previewFixture(119_000, 119_000, 0).settlement, missing: 0 }),
      // The backend's rule: no warning once the document has Arabic.
      documents_arabic_guard: (args) => {
        const { lang, secondLang } = args.input as { lang: string; secondLang: string | null };
        return lang === "ar" || secondLang === "ar" ? null : { suggested: "ar", categoryIds: [] };
      },
      documents_save_draft: (args) => {
        saved.push(args);
        return { ...draftQuoteFixture, id: "d4", docType: "invoice" };
      },
    });
    renderWithProviders(<DocumentEditor kind="invoice" draftId="d4" session={ownerSession} />);
    await userEvent.click(await screen.findByRole("button", { name: "Valider" }));
    const dialog = await screen.findByRole("dialog", { name: "Valider la facture" });
    await userEvent.click(await within(dialog).findByRole("button", { name: "Imprimer en arabe" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({ id: "d4", input: { cart: { lang: "ar" }, secondLang: "fr" } });
    await waitFor(() =>
      expect(within(dialog).queryByRole("button", { name: "Imprimer en arabe" })).not.toBeInTheDocument(),
    );
    // Never blocking: the validation stays open.
    expect(within(dialog).getByRole("button", { name: "Valider" })).toBeEnabled();
  });

  it("shows nothing when the backend has no warning", async () => {
    mockCommands({ documents_arabic_guard: () => null });
    const { container } = renderWithProviders(
      <ArabicGuardBanner
        input={{ lang: "ar", secondLang: null, productIds: [], consumer: true }}
        onSwitch={() => {}}
      />,
    );
    await waitFor(() => expect(container.textContent).toBe(""));
  });
});
