import type { DiscoveredHost } from "@/lib/ipc/types";

export interface JoinDraft {
  code: string;
  terminalName: string;
  terminalCode: string;
}

export type JoinErrors = Partial<Record<keyof JoinDraft, string>>;

/** The main PC's rule for a till series (`devices::valid_code`). */
export const TERMINAL_CODE = /^[A-Z][A-Z0-9]{0,7}$/;

const IPV4 = /^\d{1,3}(\.\d{1,3}){3}$/;

export function validateJoin(draft: JoinDraft): JoinErrors {
  const errors: JoinErrors = {};
  if (!/^\d{6}$/.test(draft.code)) errors.code = "sync:lan.join.codeInvalid";
  const name = draft.terminalName.trim();
  if (name === "") errors.terminalName = "validation:required";
  else if (name.length > 40) errors.terminalName = "validation:tooLong";
  if (!TERMINAL_CODE.test(draft.terminalCode)) errors.terminalCode = "sync:lan.join.seriesInvalid";
  return errors;
}

/** `ip:port` of a main PC found by mDNS: its first IPv4 address (what the owner would type), or
 * `null` when it announced none (the owner then types the address). */
export function hostAddress(host: DiscoveredHost): string | null {
  const ip = host.addresses.find((address) => IPV4.test(address));
  return ip === undefined ? null : `${ip}:${host.port}`;
}
