import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { LicenseStatus } from "./types";

/** This PC's licence (M9). Asked again every 10 minutes (a trial ends while the app runs) and
 * after any `license_*` refusal (`src/lib/query.ts`). The webview is not the boundary: Rust refuses
 * every write whatever this says, so a failed call leaves the screens as they are. */
export function useLicenseStatus(enabled = true) {
  return useQuery({
    queryKey: qk.license,
    queryFn: () => commands.licenseStatus(),
    enabled,
    staleTime: 60_000,
    refetchInterval: 600_000,
  });
}

export function useActivateLicense() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (key: string) => commands.licenseActivate(key),
    onSuccess: (status) => client.setQueryData<LicenseStatus>(qk.license, status),
  });
}

/** Rust opens the file dialog (`.dzlic`); `null` when it was cancelled. */
export function useImportLicenseFile() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.licenseImportFile(),
    onSuccess: (status) => {
      if (status) client.setQueryData<LicenseStatus>(qk.license, status);
    },
  });
}

/** The owner, with the PIN again. */
export function useRemoveLicense() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (pin: string) => commands.licenseRemove(pin),
    onSuccess: (status) => client.setQueryData<LicenseStatus>(qk.license, status),
  });
}
