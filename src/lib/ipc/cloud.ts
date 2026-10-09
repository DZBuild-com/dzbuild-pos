import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import type { Lang } from "@/lib/i18n";
import { commands, events } from "./client";
import { qk } from "./keys";
import type { CloudPage, PublishChange, PublishQuery } from "./types";

export function useCloudStatus() {
  return useQuery({ queryKey: qk.cloudStatus, queryFn: () => commands.cloudStatus() });
}

/** The engine's events: the status and the unseen online orders refresh without polling. */
export function useSyncEvents(): void {
  const client = useQueryClient();
  useEffect(() => {
    let cancelled = false;
    const stops: Array<() => void> = [];
    // Unsubscribed during the cleanup once in place, or as soon as it arrives after an unmount.
    const keep = (listening: Promise<() => void>) =>
      listening
        .then((stop) => (cancelled ? stop() : void stops.push(stop)))
        .catch((error: unknown) => console.error("sync listener refused:", error));
    void keep(
      events.syncStatusChanged.listen((event) => {
        client.setQueryData(qk.cloudStatus, event.payload.status);
        // the notices, failed items… again; the status is the event's own, not read once more
        void client.invalidateQueries({
          queryKey: ["cloud"],
          predicate: (query) => query.queryKey[1] !== qk.cloudStatus[1],
        });
      }),
    );
    void keep(
      events.onlineOrdersChanged.listen(() => {
        void client.invalidateQueries({ queryKey: ["onlineOrders"] });
        void client.invalidateQueries({ queryKey: qk.cloudStatus });
      }),
    );
    return () => {
      cancelled = true;
      for (const stop of stops) stop();
    };
  }, [client]);
}

export function useLinkStart() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (lang: Lang) => commands.cloudLinkStart(lang),
    onSuccess: (start) => client.setQueryData(qk.cloudAttempt, start.attempt),
  });
}

export function useDeviceFlowStart() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.cloudDeviceFlowStart(),
    onSuccess: (start) => client.setQueryData(qk.cloudAttempt, start.attempt),
  });
}

/** Polled every second while an attempt waits (the browser or the phone finishes it); once it
 * linked, the status is read again. */
export function useLinkAttempt(enabled: boolean) {
  const client = useQueryClient();
  const attempt = useQuery({
    queryKey: qk.cloudAttempt,
    queryFn: () => commands.cloudLinkPoll(),
    enabled,
    refetchInterval: (q) => {
      const state = q.state.data?.state.state;
      return state === "waiting" || state === "finishing" ? 1_000 : false;
    },
  });
  const linked = attempt.data?.state.state === "linked";
  useEffect(() => {
    if (linked) void client.invalidateQueries({ queryKey: qk.cloudStatus });
  }, [linked, client]);
  return attempt;
}

/** Fetches the store's profile and registers this till: seconds on a slow line (the wizard waits). */
export function useChooseStore() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (storeId: string) => commands.cloudLinkChoose(storeId),
    onSuccess: () => client.invalidateQueries({ queryKey: ["cloud"] }),
  });
}

/** The wizard leaves the attempt at once; the phone may have finished the link just before the
 * cancel, so the status is read again. */
export function useCancelLink() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.cloudLinkCancel(),
    onMutate: () => client.setQueryData(qk.cloudAttempt, null),
    onSettled: () => client.invalidateQueries({ queryKey: qk.cloudStatus }),
  });
}

export function useUnlink() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (pin: string) => commands.cloudUnlink(pin),
    onSuccess: () => client.invalidateQueries({ queryKey: ["cloud"] }),
  });
}

export function useSyncNow() {
  return useMutation({ mutationFn: () => commands.cloudSyncNow() });
}

export function useCloudNotices(enabled = true) {
  return useQuery({ queryKey: qk.cloudNotices, queryFn: () => commands.cloudNotices(), enabled });
}

export function useDismissNotice() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => commands.cloudNoticeDismiss(id),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.cloudNotices }),
  });
}

export function useFailedItems(enabled = true) {
  return useQuery({ queryKey: qk.cloudFailed, queryFn: () => commands.cloudOutboxFailed(), enabled });
}

export function useRetryItem() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => commands.cloudOutboxRetry(id),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.cloudFailed }),
  });
}

/** `mode` (`cloud.publish.mode`) is part of the key: the rows' ticks follow it. */
export function usePublishList(query: PublishQuery, enabled = true, mode: unknown = null) {
  return useQuery({
    queryKey: [...qk.cloudPublish(query), mode],
    queryFn: () => commands.cloudPublishList(query),
    enabled,
  });
}

export function useSetPublish() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (change: PublishChange) => commands.cloudPublishSet(change),
    onSuccess: () => client.invalidateQueries({ queryKey: ["cloud", "publish"] }),
  });
}

/** "Renvoyer tout le stock": the next sync sends the level of every product sent so far. */
export function useReconcileStock() {
  return useMutation({ mutationFn: () => commands.cloudStockReconcile(null) });
}

export function useImportOnline() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.cloudImportOnline(),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["cloud", "publish"] });
      void client.invalidateQueries({ queryKey: ["products"] });
    },
  });
}

export function useCloudDevices(enabled: boolean) {
  return useQuery({
    queryKey: qk.cloudDevices,
    queryFn: () => commands.cloudDevices(),
    enabled,
    retry: false,
  });
}

/** In the system browser, never in the app's window. */
export function useOpenCloudPage() {
  return useMutation({ mutationFn: (page: CloudPage) => commands.cloudOpenPage(page) });
}
