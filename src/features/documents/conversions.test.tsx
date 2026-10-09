import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { mockCommands, renderWithProviders } from "@/test/render";
import { documentFixture, previewFixture } from "@/test/fixtures-m1";
import { customerHitsFixture } from "@/test/fixtures-m2";
import {
  archivePageFixture,
  draftQuoteFixture,
  mentionsCompleteFixture,
  mentionsMissingFixture,
  validatedInvoiceFixture,
} from "@/test/fixtures-m3";
import { ConvertMenu } from "./ConvertMenu";
import { InvoiceDeliveriesDialog } from "./InvoiceDeliveriesDialog";
import { InvoiceTicketDialog } from "./InvoiceTicketDialog";

const quote = { ...draftQuoteFixture, id: "q-1", status: "validated" as const, number: "DV2026-000012" };

describe("conversions", () => {
  it("turns a quote into an invoice draft", async () => {
    const calls: unknown[] = [];
    mockCommands({
      documents_convert: (args) => {
        calls.push(args);
        return { ...draftQuoteFixture, id: "d9", docType: "invoice" };
      },
    });
    renderWithProviders(<ConvertMenu doc={quote} state={{ state: "open", validUntil: "2026-10-27" }} />);
    await userEvent.click(screen.getByRole("button", { name: "Transformer" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "En facture" }));
    await waitFor(() =>
      expect(calls[0]).toMatchObject({ input: { sourceId: "q-1", target: "invoice", reprice: false } }),
    );
  });

  it("offers today's prices for an expired quote and nothing for a converted one", async () => {
    const { unmount } = renderWithProviders(
      <ConvertMenu doc={quote} state={{ state: "expired", validUntil: "2026-09-01" }} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Transformer" }));
    expect(screen.getByRole("menuitem", { name: "En facture aux prix du jour" })).toBeInTheDocument();
    unmount();
    renderWithProviders(<ConvertMenu doc={quote} state={{ state: "converted", into: ["FA2026-000045"] }} />);
    expect(screen.getByRole("button", { name: "Transformer" })).toBeDisabled();
    expect(screen.getByText("Déjà transformé : FA2026-000045")).toBeInTheDocument();
  });

  it("invoices a ticket to a chosen customer", async () => {
    const calls: unknown[] = [];
    mockCommands({
      customers_search: () => customerHitsFixture,
      documents_mentions: () => mentionsCompleteFixture,
      documents_invoice_ticket: (args) => {
        calls.push(args);
        return {
          document: validatedInvoiceFixture,
          settlement: null,
          shortages: [],
          customerBalance: null,
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
    renderWithProviders(
      <InvoiceTicketDialog
        ticketId={documentFixture.id}
        ticketNumber="TK2026-000187"
        open
        onClose={() => {}}
      />,
    );
    // M2's first customer hit is "Karim Benali".
    await userEvent.type(screen.getByRole("searchbox", { name: "Client" }), "Karim");
    await userEvent.click(await screen.findByRole("option", { name: /Karim/ }));
    await userEvent.click(screen.getByRole("button", { name: "Facturer le ticket" }));
    await waitFor(() => expect(calls[0]).toMatchObject({ input: { ticketId: documentFixture.id } }));
    expect(await screen.findByRole("dialog", { name: "Imprimer FA2026-000045" })).toBeInTheDocument();
  });

  it("invoices a ticket in the second language the settings set for new documents", async () => {
    const calls: unknown[] = [];
    mockCommands({
      customers_search: () => customerHitsFixture,
      documents_get: () => ({ ...documentFixture, lang: "fr" }),
      settings_get: ({ key }) => (key === "documents.layout" ? { secondLang: "ar" } : null),
      documents_mentions: () => mentionsCompleteFixture,
      documents_invoice_ticket: (args) => {
        calls.push(args);
        return Promise.reject({ code: "unknown" });
      },
    });
    renderWithProviders(
      <InvoiceTicketDialog
        ticketId={documentFixture.id}
        ticketNumber="TK2026-000187"
        open
        onClose={() => {}}
      />,
    );
    await waitFor(() => expect(screen.getByLabelText("Deuxième langue")).toHaveValue("ar"));
    await userEvent.type(screen.getByRole("searchbox", { name: "Client" }), "Karim");
    await userEvent.click(await screen.findByRole("option", { name: /Karim/ }));
    await userEvent.click(screen.getByRole("button", { name: "Facturer le ticket" }));
    await waitFor(() => expect(calls[0]).toMatchObject({ input: { secondLang: "ar" } }));
  });

  it("shows the backend's figures of the delivery notes' invoice and waits for them to be covered", async () => {
    const asked: Array<{ deliveryIds: string[]; tenders: unknown[] }> = [];
    const made: unknown[] = [];
    const bl = {
      ...archivePageFixture.rows[0]!,
      id: "bl-1",
      docType: "delivery_note" as const,
      number: "BL2026-000007",
      totalTtc: 1_447_000,
      totalDue: 1_447_000,
      state: { state: "to_invoice" as const },
    };
    mockCommands({
      documents_search: () => ({ rows: [bl], next: null }),
      documents_mentions: () => mentionsCompleteFixture,
      documents_deliveries_preview: (args) => {
        const typed = args as { deliveryIds: string[]; tenders: unknown[] };
        asked.push(typed);
        const settlement = previewFixture(1_447_000, 0, 14_470).settlement;
        return typed.tenders.length === 0
          ? settlement
          : { ...settlement, cashTendered: 1_461_470, paidTotal: 1_461_470, missing: 0 };
      },
      documents_invoice_deliveries: (args) => {
        made.push(args);
        return Promise.reject({ code: "unknown" });
      },
    });
    renderWithProviders(<InvoiceDeliveriesDialog customerId="c-amine" onClose={() => {}} />);
    await userEvent.click(await screen.findByRole("checkbox", { name: /BL2026-000007/ }));
    expect(await screen.findByText("Net à payer")).toBeInTheDocument();
    await waitFor(() => expect(asked.at(-1)).toMatchObject({ deliveryIds: ["bl-1"], tenders: [] }));
    const submit = screen.getByRole("button", { name: "Créer la facture" });
    expect(submit).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Espèces"), "14614,70");
    await waitFor(() => expect(submit).toBeEnabled());
    await userEvent.click(submit);
    await waitFor(() => expect(made[0]).toMatchObject({ input: { deliveryIds: ["bl-1"] } }));
  });

  it("blocks the delivery notes' invoice while a mention of the customer is missing", async () => {
    const bl = {
      ...archivePageFixture.rows[0]!,
      id: "bl-1",
      docType: "delivery_note" as const,
      number: "BL2026-000007",
      state: { state: "to_invoice" as const },
    };
    mockCommands({
      documents_search: () => ({ rows: [bl], next: null }),
      documents_mentions: () => mentionsMissingFixture,
      documents_deliveries_preview: () => ({ ...previewFixture(1_447_000, 0).settlement, missing: 0 }),
    });
    renderWithProviders(<InvoiceDeliveriesDialog customerId="c-amine" onClose={() => {}} />);
    await userEvent.click(await screen.findByRole("checkbox", { name: /BL2026-000007/ }));
    expect(await screen.findByText(/Il manque sur la fiche du client/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Créer la facture" })).toBeDisabled();
  });
});
