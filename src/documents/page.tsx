import type { HostJob, HostPage } from "@/lib/ipc/types";
import { ArchiveSummaryDocument } from "./archive-summary";
import { ContractPage } from "./contract";
import { DeliveryDocument } from "./delivery-note";
import { InvoiceDocument } from "./invoice";
import { LabelSheet, sheetRtl } from "./label-sheet";
import { ProcessingRegisterDocument } from "./processing-register";
import { PurchaseDocument } from "./purchase";
import { QuoteDocument } from "./quote";
import { ReceiptDocument } from "./receipt";
import { RouteSheetPage } from "./route-sheet";
import { StatementDocument } from "./statement";
import { TableDocument } from "./table";
import { WarrantyDocument } from "./warranty";
import { ZakatDocument } from "./zakat";

/** The template of a print-host page, chosen by the page's template and the document's type. */
export function DocumentPage({ page }: { page: HostPage }) {
  switch (page.template) {
    case "document": {
      const m = page.model;
      switch (m.docType) {
        case "quote":
        case "proforma":
          return <QuoteDocument m={m} />;
        case "delivery_note":
        case "transfer":
          return <DeliveryDocument m={m} />;
        case "purchase_order":
        case "goods_receipt":
        case "supplier_invoice":
          return <PurchaseDocument m={m} />;
        default:
          return <InvoiceDocument m={m} />;
      }
    }
    case "receipt":
      return <ReceiptDocument page={page.model} />;
    case "statement":
      return <StatementDocument m={page.model} />;
    case "table":
      return <TableDocument m={page.model} />;
    case "warranty":
      return <WarrantyDocument m={page.model} />;
    case "archive_summary":
      return <ArchiveSummaryDocument m={page.model} />;
    case "label_sheet":
      return <LabelSheet sheet={page.model} />;
    case "contract":
      return <ContractPage model={page.model} />;
    case "route_sheet":
      return <RouteSheetPage model={page.model} />;
    case "zakat_statement":
      return <ZakatDocument m={page.model} />;
    case "processing_register":
      return <ProcessingRegisterDocument m={page.model} />;
    case "receipt_roll":
      // HW-002: the column is the roll less its 2 mm margins (Rust `PageSetup::roll`).
      return (
        <div className="doc-roll" style={{ inlineSize: `${page.model.widthMm - 4}mm` }}>
          <ReceiptDocument page={page.model.page} />
        </div>
      );
  }
}

/** Whether the page reads right to left (its document language, from the model). */
export function pageRtl(page: HostPage): boolean {
  if (page.template === "label_sheet") return sheetRtl(page.model);
  if (page.template === "receipt_roll") return page.model.page.receipt.rtl;
  const model = page.model as { rtl?: boolean; receipt?: { rtl: boolean } };
  return model.rtl ?? model.receipt?.rtl ?? false;
}

/** The page's language and direction, set on `<html>` by the print host. */
export function pageDirection(job: HostJob): { lang: string; dir: "rtl" | "ltr" } {
  return { lang: job.lang, dir: pageRtl(job.page) ? "rtl" : "ltr" };
}
