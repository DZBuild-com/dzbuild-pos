import { fireEvent, waitFor } from "@testing-library/react";

export interface ScannerKey {
  code: string;
  key: string;
  shiftKey?: boolean;
}

/** Keydowns as a barcode scanner sends them, 5 ms apart whatever the machine's load: the scan
 * detectors read `timeStamp`, which a busy parallel test run would otherwise spread past the
 * scanner's gaps (HW-021). */
export function scannerKeys(target: Element, keys: ScannerKey[]): void {
  // the clock the other events of the page carry (jsdom stamps events with its own time base)
  const start = new Event("probe").timeStamp;
  keys.forEach((init, i) => {
    const event = new KeyboardEvent("keydown", { ...init, bubbles: true, cancelable: true });
    Object.defineProperty(event, "timeStamp", { value: start + i * 5 });
    fireEvent(target, event);
  });
}

/** A scanned code of digits, then Enter; `keyOf` gives what the Windows layout types for each. */
export function scanDigits(target: Element, code: string, keyOf: (digit: string) => string = (d) => d): void {
  scannerKeys(target, [
    ...[...code].map((ch) => ({ code: `Digit${ch}`, key: keyOf(ch) })),
    { code: "Enter", key: "Enter" },
  ]);
}

/**
 * Scans `code` into `target` until the screen asked the backend for it (`lookups` counts the
 * `products_find_by_code` calls): a whole-app test may scan while a query is still settling the
 * screen's window scanner (the till read again after it opened), which would drop that burst.
 * A scan that reached the backend is never repeated, so a line is never added twice.
 */
export async function scanUntilLooked(target: Element, code: string, lookups: () => number): Promise<void> {
  const before = lookups();
  await waitFor(() => {
    if (lookups() === before) scanDigits(target, code);
    if (lookups() === before) throw new Error(`the scan of ${code} did not reach the backend yet`);
  });
}
