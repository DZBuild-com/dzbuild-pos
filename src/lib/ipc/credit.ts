import {
  keepPreviousData,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import type { Lang } from "@/lib/i18n";
import { useMemo } from "react";
import { commands } from "./client";
import { qk } from "./keys";
import type { CollectInput, CollectRequest, EntryPage, EntryQuery, LedgerEntry, Money } from "./types";

/** The versement's figures while the cashier types: the droit de timbre on the cash part, what
 * to receive and the balance after all come from the backend. Idle for `null` or no amount. */
export function useCreditQuote(input: CollectInput | null) {
  const idle = input === null || input.amount <= 0;
  return useQuery({
    queryKey: qk.creditQuote(input),
    queryFn: () => commands.creditQuote(input as CollectInput),
    enabled: !idle,
    placeholderData: idle ? undefined : keepPreviousData,
    staleTime: 0,
  });
}

/** After a versement or an account change: the customer, his kridi, the drawer and the reports. */
function useRefreshAccounts() {
  const client = useQueryClient();
  return () =>
    Promise.all(
      [qk.customers, ["credit"], ["cash"], ["reports"]].map((queryKey) =>
        client.invalidateQueries({ queryKey }),
      ),
    );
}

export function useCollect() {
  const refresh = useRefreshAccounts();
  return useMutation({
    mutationFn: (request: CollectRequest) => commands.creditCollect(request),
    onSuccess: refresh,
  });
}

/** A DUPLICATA of a versement receipt, in another language if asked. */
export function useReprintVersement() {
  return useMutation({
    mutationFn: ({ entryId, lang }: { entryId: string; lang: Lang | null }) =>
      commands.creditReprintReceipt(entryId, lang),
  });
}

export function useSetOpening() {
  const refresh = useRefreshAccounts();
  return useMutation({
    mutationFn: ({ customerId, amount, note }: { customerId: string; amount: Money; note: string | null }) =>
      commands.creditSetOpening(customerId, amount, note),
    onSuccess: refresh,
  });
}

export function useAdjustCredit() {
  const refresh = useRefreshAccounts();
  return useMutation({
    mutationFn: ({ customerId, amount, note }: { customerId: string; amount: Money; note: string }) =>
      commands.creditAdjust(customerId, amount, note),
    onSuccess: refresh,
  });
}

/** Entries of an account per page, newest first. */
export const ENTRY_PAGE = 500;

/** A party's history as the notebook tab reads it: newest first, older pages on demand. */
export interface EntryPages {
  entries: LedgerEntry[];
  /** Older entries exist: `loadMore` reads the next page. */
  hasMore: boolean;
  loadMore: () => void;
  loadingMore: boolean;
  /** An opening balance is set (whichever page is loaded); `null` until the first page. */
  hasOpening: boolean | null;
  isPending: boolean;
}

/** The pages of `fetchPage` chained on the last entry's id (the backend's `before`). */
export function useEntryPages(
  queryKey: readonly unknown[],
  fetchPage: (query: EntryQuery) => Promise<EntryPage>,
  enabled: boolean,
): EntryPages {
  const pages = useInfiniteQuery({
    queryKey: [...queryKey, "entries", "pages"],
    queryFn: ({ pageParam }) => fetchPage({ from: null, to: null, limit: ENTRY_PAGE, before: pageParam }),
    initialPageParam: null as string | null,
    getNextPageParam: (last: EntryPage) => (last.truncated ? (last.entries.at(-1)?.id ?? null) : null),
    enabled,
  });
  const loaded = pages.data?.pages;
  // One array per set of pages read, so the tables below keep their rows between renders.
  const entries = useMemo(() => (loaded ?? []).flatMap((page) => page.entries), [loaded]);
  return {
    entries,
    hasMore: pages.hasNextPage,
    loadMore: () => void pages.fetchNextPage(),
    loadingMore: pages.isFetchingNextPage,
    hasOpening: loaded?.[0]?.hasOpening ?? null,
    isPending: pages.isPending,
  };
}

/** A customer's kridi notebook, newest first (the latest sales and versements whatever its length). */
export function useCreditEntryPages(customerId: string | null): EntryPages {
  return useEntryPages(
    qk.credit(customerId ?? ""),
    (query) => commands.creditEntries(customerId ?? "", query),
    customerId !== null,
  );
}

/** The debts a versement can settle, oldest first. */
export function useOpenItems(customerId: string | null) {
  return useQuery({
    queryKey: [...qk.credit(customerId ?? ""), "open"],
    queryFn: () => commands.creditOpenItems(customerId ?? ""),
    enabled: customerId !== null,
  });
}

export function useStatement(customerId: string | null, from: string, to: string) {
  return useQuery({
    queryKey: [...qk.credit(customerId ?? ""), "statement", from, to],
    queryFn: () => commands.creditStatement(customerId ?? "", from, to),
    enabled: customerId !== null,
  });
}

export function usePrintStatement() {
  return useMutation({
    mutationFn: ({
      customerId,
      from,
      to,
      lang,
    }: {
      customerId: string;
      from: string;
      to: string;
      lang: Lang;
    }) => commands.creditPrintStatement(customerId, from, to, lang),
  });
}

export function useAging(asOf: string) {
  return useQuery({ queryKey: qk.creditAging(asOf), queryFn: () => commands.creditAging(asOf) });
}

export function useReceivables() {
  return useQuery({ queryKey: qk.receivables, queryFn: () => commands.creditReceivables() });
}

export function useRebuildBalances() {
  const refresh = useRefreshAccounts();
  return useMutation({ mutationFn: () => commands.creditRebuildBalances(), onSuccess: refresh });
}

/** AUD-004: every versement receipt `RV…` against its counter and the journal (`audit.view`). */
export function useVerifyReceipts() {
  return useMutation({ mutationFn: () => commands.creditVerifyReceipts() });
}
