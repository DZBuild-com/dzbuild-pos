import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { commands, events } from "./client";
import { qk } from "./keys";
import type { UpdateProgress } from "./types";

/** Every minute; every 3 s while a download runs, so a failed one (no `update-progress` at its
 * end) is shown at once. */
export function useUpdateStatus(enabled = true) {
  return useQuery({
    queryKey: qk.updates,
    queryFn: () => commands.updatesStatus(),
    enabled,
    refetchInterval: (query) => (query.state.data?.download.kind === "downloading" ? 3_000 : 60_000),
  });
}

/** A failed check is recorded in the status (`lastCheckError`): read it again then too. */
export function useCheckUpdates() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.updatesCheck(),
    onSuccess: (status) => client.setQueryData(qk.updates, status),
    onError: () => client.invalidateQueries({ queryKey: qk.updates }),
  });
}

export function useDownloadUpdate() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.updatesDownload(),
    onSuccess: (status) => client.setQueryData(qk.updates, status),
  });
}

export function useInstallUpdate() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.updatesInstall(),
    onSettled: () => client.invalidateQueries({ queryKey: qk.updates }),
  });
}

/** `update-progress` while a download runs; the status is refetched when it completes. */
export function useUpdateProgress(onProgress: (progress: UpdateProgress) => void): void {
  const client = useQueryClient();
  const latest = useRef(onProgress);
  useEffect(() => {
    latest.current = onProgress;
  });
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    events.updateProgress
      .listen((event) => {
        latest.current(event.payload);
        if (event.payload.total !== null && event.payload.received >= event.payload.total) {
          void client.invalidateQueries({ queryKey: qk.updates });
        }
      })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      .catch((error: unknown) => console.warn("update progress", error));
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [client]);
}
