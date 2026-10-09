import { type QueryClient, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { commands, events } from "./client";
import { qk } from "./keys";
import type {
  JoinInput,
  LanCloseRequested,
  LanPairProgress,
  LanStatus,
  LanStatusChanged,
  LinkState,
  SearchQuery,
} from "./types";

type Listen<T> = (handler: (event: { payload: T }) => void) => Promise<() => void>;

// Module-level: `events.x.listen` is a new function at each access, so hooks keep these instead.
const listenStatus: Listen<LanStatusChanged> = (handler) => events.lanStatusChanged.listen(handler);
const listenProgress: Listen<LanPairProgress> = (handler) => events.lanPairProgress.listen(handler);
const listenClose: Listen<LanCloseRequested> = (handler) => events.lanCloseRequested.listen(handler);

const refreshLan = (client: QueryClient) => client.invalidateQueries({ queryKey: ["lan"] });

/** Subscribes to a Tauri event for the lifetime of the component (as `useDeviceStatus`). */
function useEvent<T>(listen: Listen<T>, onEvent: (payload: T) => void) {
  const handler = useRef(onEvent);
  useEffect(() => {
    handler.current = onEvent;
  }, [onEvent]);
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    listen((event) => handler.current(event.payload))
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      .catch((error: unknown) => console.error("LAN listener refused:", error));
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [listen]);
}

/** The LAN status (UX-024): read once, then pushed by `lan-status-changed` on a till. The main PC
 * pushes nothing (its tills come and go, its notices open as they replay): it reads again every
 * 10 s, so its pill follows a till going offline or a new notice. */
export function useLanStatus(enabled = true) {
  const client = useQueryClient();
  useEvent(listenStatus, (payload) => client.setQueryData(qk.lanStatus, payload.status));
  return useQuery({
    queryKey: qk.lanStatus,
    queryFn: () => commands.lanStatus(),
    enabled,
    staleTime: 30_000,
    refetchInterval: (query) => (query.state.data?.role === "host" ? HOST_STATUS_POLL_MS : false),
  });
}

/** How often the main PC reads its LAN status again. */
export const HOST_STATUS_POLL_MS = 10_000;

/** The till journals the main PC keeps, checked on demand (Settings › Journal). */
export function useVerifyJournals() {
  return useMutation({ mutationFn: () => commands.lanVerifyJournals() });
}

/** A mutation that refreshes every LAN query when it succeeds; without an input, `mutate()` takes
 * no argument. */
function useLanMutation<O, I = void>(mutationFn: (input: I) => Promise<O>) {
  const client = useQueryClient();
  return useMutation({ mutationFn, onSuccess: () => refreshLan(client) });
}

export const useBecomeHost = () => useLanMutation(() => commands.lanBecomeHost());
export const useBecomeStandalone = () => useLanMutation(() => commands.lanBecomeStandalone());
export const usePairingCode = () => useLanMutation(() => commands.lanPairingCode());
export const useCancelPairing = () => useLanMutation(() => commands.lanCancelPairing());
export const useRevokeTill = () => useLanMutation((terminalId: string) => commands.lanRevokeTill(terminalId));
export const useAcknowledgeNotice = () => useLanMutation((id: string) => commands.lanAcknowledgeNotice(id));
export const useSyncNow = () => useLanMutation(() => commands.lanSyncNow());
export const useRetryRejected = () => useLanMutation(() => commands.lanRetryRejected());

/** This till's host entries and money calls still waiting for the main PC (or refused),
 * read again every 30 s. */
export function usePendingEntries(enabled = true) {
  return useQuery({
    queryKey: qk.lanPending,
    queryFn: () => commands.lanPendingEntries(),
    enabled,
    refetchInterval: 30_000,
  });
}

/** « Réessayer »: the refused entries go to the main PC again now; the answer is what still waits. */
export function usePendingRetry() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.lanPendingRetry(),
    onSuccess: (left) => {
      client.setQueryData(qk.lanPending, left);
      void client.invalidateQueries({ queryKey: qk.lanStatus });
    },
  });
}
export const usePromote = () => useLanMutation((pin: string) => commands.lanPromote(pin));
export const useLeave = () => useLanMutation((pin: string) => commands.lanLeave(pin));
export const useJoin = () => useMutation({ mutationFn: (input: JoinInput) => commands.lanJoin(input) });
export const useRestart = () => useMutation({ mutationFn: () => commands.lanRestart() });
export const useProbe = () => useMutation({ mutationFn: (address: string) => commands.lanProbe(address) });
export const useSearchHost = () =>
  useMutation({ mutationFn: (query: SearchQuery) => commands.lanSearchHost(query) });

/** The tills of this main PC, refreshed every 5 s (their last contact, what they still send). */
export function useTills(enabled = true) {
  return useQuery({
    queryKey: qk.lanTills,
    queryFn: () => commands.lanTills(),
    enabled,
    refetchInterval: 5_000,
  });
}

export function useTillClosures(terminalId: string | null) {
  return useQuery({
    queryKey: qk.lanClosures(terminalId ?? ""),
    queryFn: () => commands.lanTillClosures(terminalId ?? "", 60),
    enabled: terminalId !== null,
  });
}

export function useLanNotices(includeAcknowledged: boolean, enabled = true) {
  return useQuery({
    queryKey: qk.lanNotices(includeAcknowledged),
    queryFn: () => commands.lanNotices(includeAcknowledged),
    enabled,
    refetchInterval: 10_000,
  });
}

/** Windows PowerShell takes seconds to answer: asked once a minute at most. */
export function useFirewall(enabled: boolean) {
  return useQuery({
    queryKey: qk.lanFirewall,
    queryFn: () => commands.lanFirewall(),
    enabled,
    staleTime: 60_000,
  });
}

/** The main PCs answering on the network within 3 s; asked again each time. */
export function useDiscover(enabled: boolean) {
  return useQuery({
    queryKey: qk.lanDiscover,
    queryFn: () => commands.lanDiscover(),
    enabled,
    staleTime: 0,
    retry: false,
  });
}

/** Bytes of the main PC's copy received while joining (`null` before the first chunk). */
export function usePairProgress(): LanPairProgress | null {
  const [progress, setProgress] = useState<LanPairProgress | null>(null);
  useEvent(listenProgress, setProgress);
  return progress;
}

/** A till can reach its main PC. */
export const REACHABLE: readonly LinkState[] = ["online", "replaying", "behind"];

/** Whether this PC is a till, and whether its main PC answers (the host services and the routed
 * edits need it). */
export function useTillLink(): { isTill: boolean; online: boolean } {
  const status = useLanStatus();
  const client = status.data?.role === "client" ? status.data.client : null;
  return { isTill: status.data?.role === "client", online: client ? REACHABLE.includes(client.link) : false };
}

/** What the main PC owns and a till mirrors (`sync::catalogue` Shared rows). */
const REPLICA_KEYS = [
  ["products"],
  ["catalog"],
  ["stock"],
  ["customers"],
  ["credit"],
  ["suppliers"],
  ["purchases"],
  ["documents"],
  qk.company,
  ["settings"],
  qk.users,
  qk.roles,
] as const;

/** On a till, a page from the main PC refreshes what the screens show (at most once a second). */
export function useReplicaRefresh(status: LanStatus | undefined) {
  const client = useQueryClient();
  const applied = status?.role === "client" ? status.client?.appliedSeq : undefined;
  const previous = useRef(applied);
  const timer = useRef<number | null>(null);
  useEffect(() => {
    const before = previous.current;
    previous.current = applied;
    if (applied === undefined || before === undefined || applied === before || timer.current !== null) return;
    timer.current = window.setTimeout(() => {
      timer.current = null;
      for (const key of REPLICA_KEYS) void client.invalidateQueries({ queryKey: key });
    }, 1_000);
  }, [applied, client]);
  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );
}

/** The main PC's close button was held while tills are online (`lan-close-requested`). */
export function useCloseRequests(onRequest: (event: LanCloseRequested) => void) {
  useEvent(listenClose, onRequest);
}

/** "Fermer" in the main PC's close confirmation: the app exits. */
export const useConfirmClose = () => useMutation({ mutationFn: () => commands.lanConfirmClose() });
