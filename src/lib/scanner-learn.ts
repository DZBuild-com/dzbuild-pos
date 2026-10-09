import type { ScannerLayout } from "@/lib/ipc/types";
import { decodeKey, type HidSuffix, type ScanKey } from "@/lib/scanner";

export interface Learned {
  layout: ScannerLayout;
  prefix: string;
  suffix: HidSuffix;
}

/** The Code 128 of M1b's printer test page: what "Apprendre mon lecteur" expects by default. */
export const TEST_PAGE_CODE = "DZPOS-TEST-0123";

/** The most characters a scanner's prefix may have (`hardware_save` checks it too). */
export const PREFIX_MAX = 8;

const LAYOUTS: readonly ScannerLayout[] = ["us", "azerty"];
const ENDS = new Set(["Enter", "Tab", "NumpadEnter"]);

/** From the raw keys of one scan of `expected`: the layout whose decoding contains it, the
 * characters before it (the scanner's prefix, 8 at most) and how the scan ended. */
export function learnScanner(keys: readonly ScanKey[], expected: string): Learned | null {
  if (expected === "") return null;
  const endAt = keys.findIndex((k) => ENDS.has(k.key) || ENDS.has(k.code));
  const typed = endAt >= 0 ? keys.slice(0, endAt) : keys;
  const suffix: HidSuffix = endAt >= 0 ? "enter_or_tab" : "silence";
  for (const layout of LAYOUTS) {
    const text = typed.map((k) => decodeKey(k.code, k.shiftKey, layout) ?? "").join("");
    const at = text.indexOf(expected);
    if (at >= 0 && at <= PREFIX_MAX && at + expected.length === text.length) {
      return { layout, prefix: text.slice(0, at), suffix };
    }
  }
  return null;
}
