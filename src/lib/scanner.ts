import { useEffect, useRef } from "react";
import type { HidSuffix, ScannerLayout } from "@/lib/ipc/types";

export type { HidSuffix };

/** Spec §8, HW-021: ≥ 6 characters, the first six within 100 ms, keys < 50 ms apart, then Enter. */
export const SCAN_MIN_LENGTH = 6;
export const SCAN_WINDOW_MS = 100;
export const SCAN_MAX_GAP_MS = 50;

type Pair = readonly [string, string];

const DIGITS = Object.fromEntries([..."0123456789"].map((d) => [`Numpad${d}`, [d, d] as Pair]));
const NUMPAD: Record<string, Pair> = {
  ...DIGITS,
  NumpadDecimal: [".", "."],
  NumpadSubtract: ["-", "-"],
  NumpadAdd: ["+", "+"],
  NumpadMultiply: ["*", "*"],
  NumpadDivide: ["/", "/"],
};
const LETTERS = Object.fromEntries(
  [..."ABCDEFGHIJKLMNOPQRSTUVWXYZ"].map((l) => [`Key${l}`, [l.toLowerCase(), l] as Pair]),
);

/** What a scanner emulating a US keyboard types, by physical key (HID usage). */
const US: Record<string, Pair> = {
  ...LETTERS,
  ...NUMPAD,
  Digit1: ["1", "!"],
  Digit2: ["2", "@"],
  Digit3: ["3", "#"],
  Digit4: ["4", "$"],
  Digit5: ["5", "%"],
  Digit6: ["6", "^"],
  Digit7: ["7", "&"],
  Digit8: ["8", "*"],
  Digit9: ["9", "("],
  Digit0: ["0", ")"],
  Minus: ["-", "_"],
  Equal: ["=", "+"],
  Period: [".", ">"],
  Comma: [",", "<"],
  Slash: ["/", "?"],
  Semicolon: [";", ":"],
  Quote: ["'", '"'],
  BracketLeft: ["[", "{"],
  BracketRight: ["]", "}"],
  Backslash: ["\\", "|"],
  Backquote: ["`", "~"],
  Space: [" ", " "],
};

/** A scanner set to "French AZERTY" keyboard emulation: digits are shifted, A/Q, Z/W and M move. */
const AZERTY: Record<string, Pair> = {
  ...US,
  KeyQ: ["a", "A"],
  KeyA: ["q", "Q"],
  KeyW: ["z", "Z"],
  KeyZ: ["w", "W"],
  Semicolon: ["m", "M"],
  KeyM: [",", "?"],
  Comma: [";", "."],
  Period: [":", "/"],
  Slash: ["!", "§"],
  Digit1: ["&", "1"],
  Digit2: ["é", "2"],
  Digit3: ['"', "3"],
  Digit4: ["'", "4"],
  Digit5: ["(", "5"],
  Digit6: ["-", "6"],
  Digit7: ["è", "7"],
  Digit8: ["_", "8"],
  Digit9: ["ç", "9"],
  Digit0: ["à", "0"],
  Minus: [")", "°"],
};

const MAPS: Record<ScannerLayout, Record<string, Pair>> = { us: US, azerty: AZERTY };

/** The character a scanner meant, from the physical key and Shift: Windows' own layout (AZERTY,
 * Arabic) never matters. `null` for keys that type nothing (Shift, arrows…). */
export function decodeKey(code: string, shift: boolean, layout: ScannerLayout): string | null {
  const pair = MAPS[layout][code];
  return pair ? pair[shift ? 1 : 0] : null;
}

export interface ScanKey {
  code: string;
  key: string;
  shiftKey: boolean;
  timeStamp: number;
  ctrlKey?: boolean;
  altKey?: boolean;
}

const MODIFIERS = new Set(["Shift", "CapsLock", "Control", "Alt", "Meta"]);

/** NFR-005: a burst was just recognised as a scan (the scanner has sent the whole code); the E2E
 * suite measures from here to `dzpos:line-added` (`src/features/pos/cart-store.ts`), the app's own
 * share of "scan → line". One mark at a time, so a day of scans never piles them up. */
function markScanStart(): void {
  performance.clearMarks("dzpos:scan-start");
  performance.mark("dzpos:scan-start");
}

/** `suffix: "silence"`: a burst is over when no key came for this long. */
export const SILENCE_MS = 80;

export interface ScanOptions {
  layout: ScannerLayout;
  /** Characters the scanner types before each code (an AIM identifier such as `]E0`), removed. */
  prefix?: string;
  suffix?: HidSuffix;
}

export class ScanDetector {
  private chars: { ch: string; at: number }[] = [];

  constructor(private readonly options: ScanOptions) {}

  /** The burst typed so far as a code (its prefix removed), if it was a scan; starts over. */
  private finish(): string | null {
    const code = this.isScan() ? this.chars.map((c) => c.ch).join("") : null;
    this.chars = [];
    if (code === null) return null;
    markScanStart();
    const prefix = this.options.prefix ?? "";
    const stripped = prefix && code.startsWith(prefix) ? code.slice(prefix.length) : code;
    return stripped.length > 0 ? stripped : null;
  }

  /** Feeds one keydown; returns the scanned code when this key (Enter/Tab) ends a scan. */
  feed(event: ScanKey): string | null {
    if (event.ctrlKey || event.altKey) {
      this.chars = [];
      return null;
    }
    if (MODIFIERS.has(event.key)) return null;
    const last = this.chars.at(-1);
    if (last && event.timeStamp - last.at > SCAN_MAX_GAP_MS) this.chars = [];
    if (event.key === "Enter" || event.key === "Tab" || event.code === "NumpadEnter") return this.finish();
    const ch = decodeKey(event.code, event.shiftKey, this.options.layout);
    if (ch === null) {
      this.chars = [];
      return null;
    }
    this.chars.push({ ch, at: event.timeStamp });
    return null;
  }

  /** `suffix: "silence"`: the burst is over when no key came for SILENCE_MS; `now` is on the
   * clock of the keys' `timeStamp`. */
  flush(now: number): string | null {
    if (this.options.suffix !== "silence") return null;
    const last = this.chars.at(-1);
    if (!last || now - last.at < SILENCE_MS) return null;
    return this.finish();
  }

  /** `suffix: "silence"`: after `event` (a key that did not end a scan), waits SILENCE_MS and
   * hands `onCode` the burst if it was a scan. The key's own `timeStamp` is advanced by the time
   * really waited, so the check never mixes two clocks (WebView2 stamps keys with
   * `performance.now()`, other engines with `Date.now()`). Returns the timer to clear. */
  afterSilence(event: ScanKey, onCode: (code: string) => void): number | undefined {
    if (this.options.suffix !== "silence") return undefined;
    const stamp = event.timeStamp;
    const armed = performance.now();
    return window.setTimeout(() => {
      const code = this.flush(stamp + (performance.now() - armed));
      if (code !== null) onCode(code);
    }, SILENCE_MS + 5);
  }

  /** Fast keys are arriving right now: a field may be receiving a scan. */
  inBurst(now: number): boolean {
    const last = this.chars.at(-1);
    return this.chars.length >= 2 && last !== undefined && now - last.at <= SCAN_MAX_GAP_MS;
  }

  private isScan(): boolean {
    const first = this.chars[0];
    const sixth = this.chars[SCAN_MIN_LENGTH - 1];
    return first !== undefined && sixth !== undefined && sixth.at - first.at <= SCAN_WINDOW_MS;
  }
}

let guard = true;

/** HW-022 burst guard of numeric fields; tests switch it off (typing there is instant). */
export function setBurstGuard(on: boolean): void {
  guard = on;
}

export function burstGuardOn(): boolean {
  return guard;
}

/** Marks a field that reads scans itself (a serial number, an IMEI): the window scanner leaves
 * the scans typed there to it. */
export const OWN_SCAN = { "data-own-scan": "" } as const;

/** Scans anywhere in the window (UX-016, HW-022): the Enter ending a scan never reaches the
 * focused element, so a scan cannot click a button or submit a form. A field marked
 * [`OWN_SCAN`] keeps the scans typed in it. */
export function useScanner(
  onScan: (code: string) => void,
  {
    layout,
    prefix = "",
    suffix = "enter_or_tab",
    enabled = true,
  }: { layout: ScannerLayout; prefix?: string; suffix?: HidSuffix; enabled?: boolean },
): void {
  const handler = useRef(onScan);
  useEffect(() => {
    handler.current = onScan;
  }, [onScan]);
  useEffect(() => {
    if (!enabled) return undefined;
    const detector = new ScanDetector({ layout, prefix, suffix });
    let silence: number | undefined;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.target instanceof Element && event.target.closest("[data-own-scan]")) return;
      window.clearTimeout(silence);
      const code = detector.feed(event);
      if (code !== null) {
        event.preventDefault();
        event.stopPropagation();
        handler.current(code);
      } else {
        silence = detector.afterSilence(event, (ended) => handler.current(ended));
      }
    };
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => {
      window.clearTimeout(silence);
      window.removeEventListener("keydown", onKeyDown, { capture: true });
    };
  }, [layout, prefix, suffix, enabled]);
}
