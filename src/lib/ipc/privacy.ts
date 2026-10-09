import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Lang } from "@/lib/i18n";
import { commands } from "./client";
import { qk } from "./keys";
import type { ConsentChannel, ConsentSource } from "./types";

export type {
  AnonymiseReport,
  Consent,
  ConsentChannel,
  ConsentSource,
  ExportedCustomer,
  InstalledPack,
} from "./types";

export const CONSENT_CHANNELS: readonly ConsentChannel[] = ["sms", "whatsapp", "loyalty"];
export const CONSENT_SOURCES: readonly ConsentSource[] = ["counter", "form", "import", "customer_request"];

/** The till's keys, `[action, key]` in Task 17's order (`till::key_map`). */
export type KeyMapRows = Array<[string, string]>;

/** Every answer of the customer, newest first (the latest per channel is the one in force): the
 * customer's file, and the till's ready message and reminders (whether WhatsApp or SMS may be used). */
export function useConsents(customerId: string | null) {
  return useQuery({
    queryKey: qk.privacyConsents(customerId ?? ""),
    queryFn: () => commands.privacyConsents(customerId ?? ""),
    enabled: Boolean(customerId),
  });
}

export function useSetConsent() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (v: {
      customerId: string;
      channel: ConsentChannel;
      granted: boolean;
      source: ConsentSource;
      note: string | null;
    }) => commands.privacySetConsent(v.customerId, v.channel, v.granted, v.source, v.note),
    onSuccess: (_, { customerId }) =>
      Promise.all([
        client.invalidateQueries({ queryKey: qk.privacyConsents(customerId) }),
        // M2's `marketing_consent` follows the SMS and WhatsApp answers
        client.invalidateQueries({ queryKey: qk.customers }),
      ]),
  });
}

/** PDP-004: the customer's data as JSON and Excel in a folder the user picks in a Rust dialog;
 * `null` when the dialog is closed. */
export function useExportCustomerData() {
  return useMutation({
    mutationFn: (customerId: string) => commands.privacyExportCustomer(customerId),
  });
}

/** PDP-004: refused with `customer_has_open_items` (`details.open`) while anything is open. */
export function useAnonymise() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (v: { customerId: string; note: string | null }) =>
      commands.privacyAnonymise(v.customerId, v.note),
    onSuccess: () =>
      Promise.all(
        [qk.customers, qk.privacy, qk.loyalty].map((queryKey) => client.invalidateQueries({ queryKey })),
      ),
  });
}

/** PDP-005: the processing register on this PC's A4 printer, in `lang`. */
export function usePrintProcessingRegister() {
  return useMutation({ mutationFn: (lang: Lang) => commands.privacyRegister(lang) });
}

/** UX-017: the defaults overridden by `till.key_map`. */
export function useTillKeyMap() {
  return useQuery({ queryKey: qk.tillKeyMap, queryFn: () => commands.tillKeyMap() });
}

/** Sends the actions whose key is not the default (`{}` puts every default back). */
export function useSetTillKeyMap() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (map: Record<string, string>) => commands.tillSetKeyMap(map),
    onSuccess: (rows) => client.setQueryData(qk.tillKeyMap, rows),
  });
}

/** PLT-024: the signed packs installed on this PC, newest version first. */
export function useInstalledPacks() {
  return useQuery({
    queryKey: qk.datapacksInstalled,
    queryFn: () => commands.datapackInstalled(),
  });
}

/** Asks for the pack's `.json`, then its `.sig` (Rust dialogs), verifies the signature and
 * installs it for the next start; `null` when a dialog is closed. */
export function useInstallPack() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.datapackInstall(),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.datapacksInstalled }),
  });
}

/** Restarts the app (600 ms later, M0's `restart_soon`, `settings.company`): an installed pack
 * applies then. */
export function useRestartApp() {
  return useMutation({ mutationFn: () => commands.appRestart() });
}
