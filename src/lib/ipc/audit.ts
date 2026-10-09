import { useInfiniteQuery, useMutation } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { AuditFilter } from "./types";

export function useAuditLog() {
  return useInfiniteQuery({
    queryKey: qk.audit,
    queryFn: ({ pageParam }) => commands.auditList({ beforeSeq: pageParam, limit: 50 }),
    initialPageParam: null as number | null,
    getNextPageParam: (last) => last.nextBeforeSeq,
  });
}

export function useVerifyChain() {
  return useMutation({ mutationFn: () => commands.auditVerifyChain() });
}

/** One record's history or one family of actions (AUD-002), newest first. */
export function useAuditHistory(filter: Omit<AuditFilter, "beforeSeq" | "limit">, enabled = true) {
  return useInfiniteQuery({
    queryKey: qk.auditHistory(filter),
    queryFn: ({ pageParam }) => commands.auditHistory({ ...filter, beforeSeq: pageParam, limit: 50 }),
    initialPageParam: null as number | null,
    getNextPageParam: (last) => last.nextBeforeSeq,
    enabled,
  });
}
