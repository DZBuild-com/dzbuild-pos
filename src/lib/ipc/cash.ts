import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Lang } from "@/lib/i18n";
import { usePrinterStatus } from "@/lib/printer-status";
import { commands } from "./client";
import { qk } from "./keys";
import type { CashMoveInput, CloseInput, Money } from "./types";

function useRefreshCash() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: ["cash"] });
}

export function useCurrentSession(enabled = true) {
  return useQuery({
    queryKey: qk.cashCurrent,
    queryFn: () => commands.cashCurrent(),
    enabled,
    staleTime: 10_000,
  });
}

export function useOpenSession() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (openingFloat: Money) => commands.cashOpenSession(openingFloat),
    onSuccess: (session) => {
      client.setQueryData(qk.cashCurrent, session);
      void client.invalidateQueries({ queryKey: ["cash"], predicate: (q) => q.queryKey[1] !== "current" });
      // An open drawer blocks the update's installation.
      void client.invalidateQueries({ queryKey: qk.updates });
    },
  });
}

export function useCashMove() {
  const refresh = useRefreshCash();
  return useMutation({ mutationFn: (input: CashMoveInput) => commands.cashMove(input), onSuccess: refresh });
}

export function useCashMovements(sessionId: string | null) {
  return useQuery({
    queryKey: qk.cashMovements(sessionId ?? ""),
    queryFn: () => commands.cashMovements(sessionId ?? ""),
    enabled: sessionId !== null,
  });
}

export function useXReport(enabled: boolean) {
  return useQuery({ queryKey: qk.xReport, queryFn: () => commands.cashXReport(), enabled, staleTime: 0 });
}

export function useCloseSession() {
  const client = useQueryClient();
  const refresh = useRefreshCash();
  return useMutation({
    mutationFn: (input: CloseInput) => commands.cashCloseSession(input),
    // The closed drawer no longer blocks the update: the offer after the Z reads the blockers again.
    onSuccess: async () => {
      await Promise.all([refresh(), client.invalidateQueries({ queryKey: qk.updates })]);
    },
  });
}

/** A Z on a till whose operations have not all reached the main PC (manager approval). */
export function useCloseOffline() {
  const client = useQueryClient();
  const refresh = useRefreshCash();
  return useMutation({
    mutationFn: (input: CloseInput) => commands.cashCloseOffline(input),
    // As after any Z: the closed drawer no longer blocks the update offered after it.
    onSuccess: async () => {
      await Promise.all([refresh(), client.invalidateQueries({ queryKey: qk.updates })]);
    },
  });
}

export function useZReport(sessionId: string | null) {
  return useQuery({
    queryKey: qk.zReport(sessionId ?? ""),
    queryFn: () => commands.cashZReport(sessionId ?? ""),
    enabled: sessionId !== null,
    staleTime: Infinity,
  });
}

export function useCashSessions(enabled: boolean) {
  return useQuery({ queryKey: qk.cashSessions, queryFn: () => commands.cashSessions(60), enabled });
}

export function usePrintZ() {
  return useMutation({
    mutationFn: ({ sessionId, lang }: { sessionId: string; lang: Lang }) =>
      commands.cashPrintZ(sessionId, lang),
    onSuccess: () => usePrinterStatus.getState().printed(),
  });
}

export function usePrintX() {
  return useMutation({
    mutationFn: (lang: Lang) => commands.cashPrintX(lang),
    onSuccess: () => usePrinterStatus.getState().printed(),
  });
}

export function useVerifyClosures() {
  return useMutation({ mutationFn: () => commands.cashVerifyClosures() });
}
