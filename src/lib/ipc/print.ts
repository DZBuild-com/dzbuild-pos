import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type {
  PdfDocumentInput,
  PdfExportRequest,
  PrintDocumentInput,
  StatementPrintInput,
  WarrantyInput,
} from "./types";

/** A print or a PDF of a document counts as one more copy in its archive row (the first one is
 * the original, every next one a DUPLICATA). */
function useInvalidating<TVars, TData>(fn: (vars: TVars) => Promise<TData>) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: () => client.invalidateQueries({ queryKey: ["archive"] }),
  });
}

/** Prints a document on this PC's printer of its kind (or the one and the copies chosen); resolves
 * `{ printer, copies, duplicate, dialog }`. */
export const usePrintDocument = () =>
  useInvalidating((input: PrintDocumentInput) => commands.documentsPrint(input));

/** Saves a document as a PDF where the Rust save dialog says; resolves `{ path, duplicate }`, or
 * `null` when the user cancels. */
export const useExportDocumentPdf = () =>
  useInvalidating((input: PdfDocumentInput) => commands.documentsExportPdf(input));

/** The page of a document for the in-app preview (drafts included, marked BROUILLON); `null` asks
 * nothing. */
export function usePrintModel(input: PdfDocumentInput | null) {
  return useQuery({
    queryKey: qk.printModel(input),
    queryFn: () => commands.documentsPrintModel(input as PdfDocumentInput),
    enabled: input !== null,
  });
}

/** Prints the A4 statement of a customer or a supplier over a period. */
export function usePrintStatementA4() {
  return useMutation({ mutationFn: (input: StatementPrintInput) => commands.statementsPrint(input) });
}

/** Saves the A4 statement as a PDF where the Rust save dialog says; `null` when the user cancels. */
export function useExportStatementPdf() {
  return useMutation({ mutationFn: (input: StatementPrintInput) => commands.statementsExportPdf(input) });
}

/** Prints the warranty certificate of a ticket or an invoice line. */
export function usePrintWarranty() {
  return useMutation({ mutationFn: (input: WarrantyInput) => commands.warrantyPrint(input) });
}

/** Saves the warranty certificate as a PDF where the Rust save dialog says; `null` when the user
 * cancels. */
export function useExportWarrantyPdf() {
  return useMutation({ mutationFn: (input: WarrantyInput) => commands.warrantyExportPdf(input) });
}

/** The PDF of any table (the `onPdf` of every toolbar); resolves `{ path, rows }`, or `null` when
 * the user cancels. */
export function useExportPdf() {
  return useMutation({ mutationFn: (request: PdfExportRequest) => commands.exportsPdf(request) });
}
