import { useInfiniteQuery, useMutation, useQuery } from "@tanstack/react-query";
import type { Lang } from "@/lib/i18n";
import { usePrinterStatus } from "@/lib/printer-status";
import { commands } from "./client";
import { qk } from "./keys";
import type { DocumentQuery, DocumentSummary } from "./types";

const PAGE = 100;

/** The filters of the sales history; the pages are keyed on the last row (`validatedAt`, `id`). */
export type DocumentFilter = Omit<DocumentQuery, "beforeValidatedAt" | "beforeId" | "limit">;

type PageStart = { validatedAt: number; id: string } | null;

function nextPage(last: DocumentSummary[]): PageStart {
  const row = last.length === PAGE ? last[last.length - 1] : undefined;
  return row?.validatedAt != null ? { validatedAt: row.validatedAt, id: row.id } : null;
}

/** Validated documents, newest first, 100 at a time (SAL-056). */
export function useDocuments(filter: DocumentFilter) {
  return useInfiniteQuery({
    queryKey: qk.documents(filter),
    queryFn: ({ pageParam }) =>
      commands.documentsList({
        ...filter,
        beforeValidatedAt: pageParam?.validatedAt ?? null,
        beforeId: pageParam?.id ?? null,
        limit: PAGE,
      }),
    initialPageParam: null as PageStart,
    getNextPageParam: nextPage,
  });
}

export function useDocument(id: string | null) {
  return useQuery({
    queryKey: qk.document(id ?? ""),
    queryFn: () => commands.documentsGet(id ?? ""),
    enabled: id !== null,
  });
}

/** A ticket number typed or scanned for a return: looked up every time. */
export function useFindDocument() {
  return useMutation({ mutationFn: (number: string) => commands.documentsFind(number) });
}

/** A DUPLICATA of a validated document, in its own language or `lang` (DOC-018). The printer
 * works: the dot clears, and the document is no longer a receipt to reprint. */
export function useReprint() {
  return useMutation({
    mutationFn: ({ id, lang }: { id: string; lang: Lang | null }) => commands.documentsReprint(id, lang),
    onSuccess: (_, { id }) => usePrinterStatus.getState().printed(id),
  });
}

export function useVerifyDocuments() {
  return useMutation({ mutationFn: () => commands.documentsVerifyChain() });
}
