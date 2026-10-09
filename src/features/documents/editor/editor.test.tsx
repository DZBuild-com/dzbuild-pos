import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Toaster } from "sonner";
import { describe, expect, it } from "vitest";
import { mockCommands, renderWithProviders } from "@/test/render";
import { ownerSession } from "@/test/fixtures";
import { previewFixture, scanHitFixture } from "@/test/fixtures-m1";
import {
  draftPreviewFixture,
  draftQuoteFixture,
  mentionsCompleteFixture,
  mentionsMissingFixture,
  validatedInvoiceFixture,
} from "@/test/fixtures-m3";
import { DocumentEditor } from "./DocumentEditor";

/** An amount as `Amount` prints it, with its exact invisible spaces (Testing Library turns them
 * into plain spaces in the text it compares, never in the string it is given). */
const amount = (text: string) => (_: string, element: Element | null) =>
  element?.tagName === "BDI" && element.textContent === text;

describe("document editor", () => {
  it("prices a quote with the backend and saves it as a draft", async () => {
    const saved: Array<{ id: string | null; input: { docType: string; cart: { lines: unknown[] } } }> = [];
    mockCommands({
      documents_mentions: () => mentionsCompleteFixture,
      products_find_by_code: () => scanHitFixture,
      products_search: () => [],
      documents_preview_draft: () => draftPreviewFixture,
      documents_save_draft: (args) => {
        saved.push(args as (typeof saved)[number]);
        return draftQuoteFixture;
      },
    });
    renderWithProviders(<DocumentEditor kind="quote" draftId={null} session={ownerSession} />);
    const scan = await screen.findByRole("searchbox", { name: "Scanner ou chercher un produit" });
    await userEvent.type(scan, "6130000000017{Enter}");
    expect(await screen.findByText("Lait Candia 1 L")).toBeInTheDocument();
    // The line's total as the backend priced it (the one-line document's TTC repeats it below).
    expect(
      await within(screen.getByRole("table")).findByText(amount("1\u202F190,00\u00A0DA")),
    ).toBeInTheDocument();
    await userEvent.keyboard("{Control>}s{/Control}");
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({ id: null, input: { docType: "quote" } });
    expect(saved[0]!.input.cart.lines).toHaveLength(1);
  });

  it("starts a new document in the second language Réglages › Documents sets (DOC-034)", async () => {
    const saved: Array<{ input: { secondLang: string | null } }> = [];
    mockCommands({
      settings_get: ({ key }) => (key === "documents.layout" ? { secondLang: "ar" } : null),
      documents_mentions: () => mentionsCompleteFixture,
      products_find_by_code: () => scanHitFixture,
      products_search: () => [],
      documents_preview_draft: () => draftPreviewFixture,
      documents_save_draft: (args) => {
        saved.push(args as (typeof saved)[number]);
        return draftQuoteFixture;
      },
    });
    renderWithProviders(<DocumentEditor kind="quote" draftId={null} session={ownerSession} />);
    const scan = await screen.findByRole("searchbox", { name: "Scanner ou chercher un produit" });
    expect(screen.getByLabelText("Deuxième langue")).toHaveValue("ar");
    await userEvent.type(scan, "6130000000017{Enter}");
    expect(await screen.findByText("Lait Candia 1 L")).toBeInTheDocument();
    await userEvent.keyboard("{Control>}s{/Control}");
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]!.input.secondLang).toBe("ar");
  });

  it("blocks the validation of an invoice while a mention is missing", async () => {
    mockCommands({
      documents_draft_cart: () => ({
        id: "d2",
        docType: "invoice",
        cart: { lines: [], priceLevelId: null, customerId: "c-amine", lang: "fr", note: null },
        validUntil: null,
        dueDate: null,
        secondLang: null,
        origin: null,
      }),
      documents_mentions: () => mentionsMissingFixture,
      documents_preview_draft: () => ({ ...draftPreviewFixture, mentions: mentionsMissingFixture }),
      customers_get: () => ({ id: "c-amine", name: "SARL Amine" }),
    });
    renderWithProviders(<DocumentEditor kind="invoice" draftId="d2" session={ownerSession} />);
    expect(await screen.findByText("Il manque sur la fiche de la boutique : NIS")).toBeInTheDocument();
    expect(screen.getByText("Il manque sur la fiche du client : RC, NIF")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Valider" })).toBeDisabled();
    expect(screen.getByRole("link", { name: "Compléter le client" })).toHaveAttribute(
      "href",
      "/customers/c-amine",
    );
  });

  it("validates an invoice with cash and credit and opens the print dialog", async () => {
    const validated: unknown[] = [];
    const settlement = { ...previewFixture(2_339_500, 544_900, 5_400).settlement, missing: 0 };
    mockCommands({
      documents_draft_cart: () => ({
        id: "d3",
        docType: "invoice",
        cart: { lines: [], priceLevelId: null, customerId: "c-amine", lang: "fr", note: null },
        validUntil: null,
        dueDate: "2026-10-28",
        secondLang: null,
        origin: null,
      }),
      documents_mentions: () => mentionsCompleteFixture,
      documents_preview_draft: () => draftPreviewFixture,
      customers_get: () => ({ id: "c-amine", name: "SARL Amine" }),
      documents_payment_preview: () => settlement,
      documents_validate: (args) => {
        validated.push(args);
        return {
          document: validatedInvoiceFixture,
          settlement,
          shortages: [],
          customerBalance: 1_800_000,
          mentions: mentionsCompleteFixture,
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
    // The app shell's toaster shows the "validé" toast.
    renderWithProviders(
      <>
        <DocumentEditor kind="invoice" draftId="d3" session={ownerSession} />
        <Toaster />
      </>,
    );
    await userEvent.click(await screen.findByRole("button", { name: "Valider" }));
    const dialog = await screen.findByRole("dialog", { name: "Valider la facture" });
    await userEvent.type(within(dialog).getByLabelText("Espèces"), "5449");
    await userEvent.type(within(dialog).getByLabelText("À terme (compte client)"), "18000");
    expect(await within(dialog).findByText(amount("54,00\u00A0DA"))).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Valider" }));
    await waitFor(() => expect(validated).toHaveLength(1));
    expect(validated[0]).toMatchObject({
      input: {
        id: "d3",
        payments: [
          { method: "cash", amount: 544_900 },
          { method: "credit", amount: 1_800_000 },
        ],
      },
    });
    expect(await screen.findByText("Document FA2026-000045 validé.")).toBeInTheDocument();
    expect(await screen.findByRole("dialog", { name: "Imprimer FA2026-000045" })).toBeInTheDocument();
  });
});
