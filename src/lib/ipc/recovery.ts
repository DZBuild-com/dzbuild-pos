import { useMutation, useQuery } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { RecoveryRestoreInput } from "./types";

/** The fatal screen's backups (local, second copy, USB sticks); refreshed every 5 s so a stick
 * plugged in shows up. */
export function useRecoveryBackups(enabled: boolean) {
  return useQuery({
    queryKey: qk.recovery,
    queryFn: () => commands.recoveryList(),
    enabled,
    refetchInterval: 5_000,
  });
}

export function useRecoveryPreview(id: string | null) {
  return useQuery({
    queryKey: qk.recoveryPreview(id ?? ""),
    queryFn: () => commands.recoveryInspect(id ?? ""),
    enabled: id !== null,
    staleTime: Infinity,
  });
}

/** Owner PIN checked against the backup, damaged files moved aside, then the app restarts. */
export function useRecoveryRestore() {
  return useMutation({ mutationFn: (input: RecoveryRestoreInput) => commands.recoveryRestore(input) });
}

/** Finding 10: the shop's database is gone while its backups are there; the owner (PIN of the
 * chosen backup) starts an empty shop anyway, journaled in it. The app restarts. */
export function useRecoveryStartFresh() {
  return useMutation({ mutationFn: (input: RecoveryRestoreInput) => commands.recoveryStartFresh(input) });
}

/** A till sets its damaged files aside (never deleted) and restarts as a new PC that
 * joins its main PC again. */
export function useRecoveryRejoin() {
  return useMutation({ mutationFn: () => commands.recoveryRejoin() });
}
