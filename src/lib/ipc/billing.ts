import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type {
  ArchiveCursor,
  ArchiveQuery,
  ConvertInput,
  CreditNoteInput,
  DeliveriesInput,
  DraftInput,
  GuardInput,
  PaymentInput,
  TicketInvoiceInput,
  ValidateInput,
} from "./types";

/** What a commercial document write can change: the documents and their archive, the stock, the
 * drawer, the customers' accounts and every report. */
const TOUCHED = [["documents"], ["archive"], ["stock"], ["cash"], ["credit"], ["reports"]] as const;

function useInvalidating<TVars, TData>(fn: (vars: TVars) => Promise<TData>) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: () => Promise.all(TOUCHED.map((key) => client.invalidateQueries({ queryKey: key }))),
  });
}

/** The archive's filters; an empty (or absent) kind list means every kind the user may see. */
export type ArchiveFilter = Partial<Omit<ArchiveQuery, "before" | "limit">>;

const NO_FILTER: Omit<ArchiveQuery, "before" | "limit"> = {
  kinds: [],
  status: null,
  text: null,
  customerId: null,
  supplierId: null,
  from: null,
  to: null,
  minTotal: null,
  maxTotal: null,
};

/** Every document of the kinds the user may see, newest first, 100 at a time (DOC-017);
 * `enabled: false` asks nothing. */
export function useArchive(filter: ArchiveFilter, enabled = true) {
  return useInfiniteQuery({
    queryKey: qk.archive(filter),
    queryFn: ({ pageParam }) =>
      commands.documentsSearch({ ...NO_FILTER, ...filter, before: pageParam, limit: 100 }),
    initialPageParam: null as ArchiveCursor | null,
    getNextPageParam: (last) => last.next ?? null,
    enabled,
  });
}

/** The editor's figures and missing mentions, all computed by the backend; `null` asks nothing. */
export function useDraftPreview(input: DraftInput | null) {
  return useQuery({
    queryKey: qk.draftPreview(input),
    queryFn: () => commands.documentsPreviewDraft(input as DraftInput),
    enabled: input !== null,
    placeholderData: (previous) => previous,
  });
}

export const useSaveDraft = () =>
  useInvalidating(({ id, input }: { id: string | null; input: DraftInput }) =>
    commands.documentsSaveDraft(id, input),
  );
export const useValidateDocument = () =>
  useInvalidating((input: ValidateInput) => commands.documentsValidate(input));
export const useConvert = () => useInvalidating((input: ConvertInput) => commands.documentsConvert(input));
export const useInvoiceDeliveries = () =>
  useInvalidating((input: DeliveriesInput) => commands.documentsInvoiceDeliveries(input));
export const useInvoiceTicket = () =>
  useInvalidating((input: TicketInvoiceInput) => commands.documentsInvoiceTicket(input));
export const useCreateCreditNote = () =>
  useInvalidating((input: CreditNoteInput) => commands.documentsCreateCreditNote(input));
export const useCancelDraft = () => useInvalidating((id: string) => commands.documentsCancelDraft(id));
export const useLinkSale = () =>
  useInvalidating(({ quoteId, ticketId }: { quoteId: string; ticketId: string }) =>
    commands.documentsLinkSale(quoteId, ticketId),
  );

/** SAL-046: a validated, unexpired quote or proforma as a till cart (nothing written). */
export const useOfferCart = () =>
  useMutation({ mutationFn: (id: string) => commands.documentsOfferCart(id) });

/** The validation dialog's settlement: duty on the cash part, change, what is left. */
export function usePaymentPreview(id: string | null, tenders: PaymentInput[] | null) {
  return useQuery({
    queryKey: qk.paymentPreview(id ?? "", tenders),
    queryFn: () => commands.documentsPaymentPreview(id ?? "", tenders ?? []),
    enabled: id !== null && tenders !== null,
    placeholderData: (previous) => previous,
  });
}

/** DOC-031: the figures of the invoice of the ticked delivery notes with the tenders typed so
 * far (TTC, duty on the cash part, net to pay, what is left, change): the backend's, nothing
 * written. Idle while no note is ticked. */
export function useDeliveriesPreview(deliveryIds: string[], tenders: PaymentInput[]) {
  return useQuery({
    queryKey: qk.deliveriesPreview(deliveryIds, tenders),
    queryFn: () => commands.documentsDeliveriesPreview(deliveryIds, tenders),
    enabled: deliveryIds.length > 0,
    placeholderData: (previous) => previous,
  });
}

/** The figures of the credit note being typed (nothing written). */
export function useCreditQuote(input: CreditNoteInput | null) {
  return useQuery({
    queryKey: qk.creditNoteQuote(input),
    queryFn: () => commands.documentsCreditQuote(input as CreditNoteInput),
    enabled: input !== null,
    placeholderData: (previous) => previous,
  });
}

export const useDraftCart = (id: string | null) =>
  useQuery({
    queryKey: qk.draftCart(id ?? ""),
    queryFn: () => commands.documentsDraftCart(id ?? ""),
    enabled: id !== null,
  });
export const useCreditable = (invoiceId: string | null) =>
  useQuery({
    queryKey: qk.creditable(invoiceId ?? ""),
    queryFn: () => commands.documentsCreditable(invoiceId ?? ""),
    enabled: invoiceId !== null,
  });
export const useDocState = (id: string | null) =>
  useQuery({
    queryKey: qk.docState(id ?? ""),
    queryFn: () => commands.documentsState(id ?? ""),
    enabled: id !== null,
  });
export const useDocLinks = (id: string | null) =>
  useQuery({
    queryKey: qk.docLinks(id ?? ""),
    queryFn: () => commands.documentsLinks(id ?? ""),
    enabled: id !== null,
  });
export const useMentions = (customerId: string | null, enabled = true) =>
  useQuery({
    queryKey: qk.mentions(customerId),
    queryFn: () => commands.documentsMentions(customerId),
    enabled,
  });
export const useArabicGuard = (input: GuardInput | null) =>
  useQuery({
    queryKey: qk.arabicGuard(input),
    queryFn: () => commands.documentsArabicGuard(input as GuardInput),
    enabled: input !== null,
  });
export const useWarrantyLines = (documentId: string | null) =>
  useQuery({
    queryKey: qk.warranty(documentId ?? ""),
    queryFn: () => commands.documentsWarrantyLines(documentId ?? ""),
    enabled: documentId !== null,
  });
