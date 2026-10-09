import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Lang } from "@/lib/i18n";
import { commands } from "./client";
import { qk } from "./keys";

/** Whether this run is the demo shop (with its PINs then) and whether one exists on this PC. */
export function useDemoStatus() {
  return useQuery({ queryKey: qk.demo, queryFn: () => commands.demoStatus(), staleTime: Infinity });
}

/** Seeds the demo shop once in `lang`, then the app restarts on it (no invalidation). */
export function useEnterDemo() {
  return useMutation({ mutationFn: (lang: Lang) => commands.demoEnter(lang) });
}

/** Back to the real shop: the app restarts (no invalidation). */
export function useLeaveDemo() {
  return useMutation({ mutationFn: () => commands.demoLeave() });
}

/** Deletes the demo folder; the app keeps running, so the status is read again. */
export function useEraseDemo() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.demoErase(),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.demo }),
  });
}

/** DAT-013: Rust asks for the `.dzpos` file of the other PC; `true` when its restore is scheduled
 * and the app restarts (no invalidation), `false` when the dialog was cancelled. */
export function useImportFirstRun() {
  return useMutation({ mutationFn: () => commands.setupImportBackup() });
}
