import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { commands, events } from "./client";
import { qk } from "./keys";
import type { ArchiveInput, ArchiveProgress } from "./types";

/** The G50 figures of a month (`YYYY-MM`), computed by the backend; `null` asks nothing. */
export function useG50(month: string | null) {
  return useQuery({
    queryKey: qk.g50(month),
    queryFn: () => commands.fiscalG50(month ?? ""),
    enabled: month !== null,
  });
}

/** Writes the fiscal archive of a period where the Rust save dialog says; resolves the manifest's
 * files and chains, or `null` when the user cancels. The export is anchored in the journal. */
export function useExportArchive() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (input: ArchiveInput) => commands.fiscalExportArchive(input),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.audit }),
  });
}

/** The stages of the archive being written (`archive-progress`): `onProgress` receives each one. */
export function useArchiveProgress(onProgress: (progress: ArchiveProgress) => void): void {
  const handler = useRef(onProgress);
  useEffect(() => {
    handler.current = onProgress;
  }, [onProgress]);
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    void events.archiveProgress
      .listen((event) => handler.current(event.payload))
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      // Without the progress the archive is still written and its result arrives: log the refusal.
      .catch((error: unknown) => console.error("archive-progress listener refused:", error));
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);
}

/** Checks an archive picked in the Rust open dialog; resolves the files that no longer match their
 * hash, or `null` when the user cancels. */
export function useVerifyArchive() {
  return useMutation({ mutationFn: () => commands.fiscalVerifyArchive() });
}
