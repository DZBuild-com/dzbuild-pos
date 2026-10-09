import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { LogLevel, SupportChannel } from "./types";

/** This PC as support needs to know it, the last crash and the log level (`settings.data`). */
export function useDiagnostics() {
  return useQuery({ queryKey: qk.diagnostics, queryFn: () => commands.diagnosticsInfo() });
}

/** Rust opens the save dialog and writes the zip; `null` when the owner cancels it. */
export function useExportDiagnostic() {
  return useMutation({ mutationFn: () => commands.diagnosticsExport() });
}

/** Applied to the running log at once and saved for this PC. */
export function useSetLogLevel() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (level: LogLevel) => commands.diagnosticsSetLogLevel(level),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.diagnostics }),
  });
}

/** The last crash `crash.log` recorded, for every signed-in user (About). */
export function useLastCrash() {
  return useQuery({ queryKey: qk.lastCrash, queryFn: () => commands.appLastCrash() });
}

/** The shop's support contact (`support.contact`), for every signed-in user. */
export function useSupportContact() {
  return useQuery({ queryKey: qk.support, queryFn: () => commands.supportContact() });
}

/** Rust opens WhatsApp or the support page in the browser; the webview only names the channel. */
export function useOpenSupport() {
  return useMutation({ mutationFn: (channel: SupportChannel) => commands.supportOpen(channel) });
}
