import { useMutation, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";

/** Packs a fresh backup as a `.dzpos` file where the user says; resolves the path, or `null`. */
export function useExportBackupFile() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.backupExportFile(),
    // The fresh backup joins the list, and the banner clears.
    onSuccess: () => client.invalidateQueries({ queryKey: qk.backups }),
  });
}

/** Places a `.dzpos` file in the backups folder, where the restore list shows it; resolves the
 * new backup, or `null` when the user cancels. */
export function useImportBackupFile() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.backupImportFile(),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.backups }),
  });
}

/** Every table as CSV files in a zip where the user says; resolves `{ path, rows }`, or `null`. */
export function useExportAllData() {
  return useMutation({ mutationFn: () => commands.dataExportAll() });
}
