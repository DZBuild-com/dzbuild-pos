import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";

export function useBackups() {
  return useQuery({ queryKey: qk.backups, queryFn: () => commands.backupList() });
}

/** How often the banner asks again: the automatic backup runs in the background after startup. */
const STATUS_REFRESH_MS = 60_000;

/** The automatic backup's outcome and the newest backup's age, for every signed-in user. */
export function useBackupStatus() {
  return useQuery({
    queryKey: qk.backupStatus,
    queryFn: () => commands.backupStatus(),
    refetchInterval: STATUS_REFRESH_MS,
  });
}

export function useBackupNow() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.backupNow(),
    // `backups` also prefixes the status key: the banner clears after a manual backup.
    onSuccess: () => client.invalidateQueries({ queryKey: qk.backups }),
  });
}

export function useRestoreBackup() {
  return useMutation({
    mutationFn: ({ fileName, pin }: { fileName: string; pin: string }) =>
      commands.backupRestore(fileName, pin),
  });
}

/** This PC's second copy (folder, stick plugged in or not, last copy, last error). */
export function useMirrorStatus() {
  return useQuery({
    queryKey: qk.mirror,
    queryFn: () => commands.backupMirrorStatus(),
    refetchInterval: 15_000,
  });
}

/** Rust opens the folder dialog; `null` when the owner cancels it. */
export function useChooseMirror() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.backupMirrorChoose(),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.backups }),
  });
}

export function useClearMirror() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.backupMirrorClear(),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.backups }),
  });
}

/** Polled while the "USB stick" card is open: a stick plugged in shows up within 3 s. */
export function useDrives(enabled: boolean) {
  return useQuery({
    queryKey: qk.drives,
    queryFn: () => commands.backupDrives(),
    enabled,
    refetchInterval: 3_000,
  });
}

export function useBackupToDrive() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (mount: string) => commands.backupToDrive(mount),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.backups }),
  });
}

/** What a backup holds (shop, last ticket, counts, owners), read without changing it. */
export function useBackupPreview(fileName: string | null) {
  return useQuery({
    queryKey: qk.backupPreview(fileName ?? ""),
    queryFn: () => commands.backupInspect(fileName ?? ""),
    enabled: fileName !== null,
    staleTime: Infinity,
  });
}

/** The integrity check of the live database; a failure raises the "database damaged" alert. */
export function useCheckDatabase() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.backupCheckDatabase(),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.backupStatus }),
  });
}
