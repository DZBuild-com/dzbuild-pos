import { type ComponentProps, type KeyboardEvent, useEffect, useRef, useState } from "react";
import { Input } from "@/components/ui/input";
import { useFieldSerialScans } from "@/lib/ipc/devices";
import { useScannerOptions } from "@/lib/ipc/hardware";
import { ScanDetector, burstGuardOn } from "@/lib/scanner";

/** Feeds a field's keydowns to a scanner detector reading the physical keys with the scanner the
 * hub configured (HW-021/022): its layout (an AZERTY or Arabic Windows never garbles a scanned
 * code), the prefix it sends (removed) and how a code ends (Enter/Tab, or a silence). The
 * returned handler answers `true` when the key ended a scan: its Enter is consumed and `onScan`
 * gets the code; a code ended by a silence reaches `onScan` once the silence is over. Off with the
 * burst guard (tests type instantly, as fast as a scanner). */
export function useScanField(onScan: (code: string) => void): (event: KeyboardEvent<HTMLElement>) => boolean {
  const options = useScannerOptions();
  const key = `${options.layout}|${options.prefix}|${options.suffix}`;
  const detector = useRef<{ key: string; detector: ScanDetector } | null>(null);
  const silence = useRef<number | undefined>(undefined);
  const handler = useRef(onScan);
  useEffect(() => {
    handler.current = onScan;
  }, [onScan]);
  useEffect(() => () => window.clearTimeout(silence.current), []);
  return (event) => {
    if (!burstGuardOn()) return false;
    if (detector.current?.key !== key) {
      detector.current = { key, detector: new ScanDetector(options) };
    }
    window.clearTimeout(silence.current);
    const code = detector.current.detector.feed(event);
    if (code === null) {
      silence.current = detector.current.detector.afterSilence(event, (ended) => handler.current(ended));
      return false;
    }
    event.preventDefault();
    event.stopPropagation();
    onScan(code);
    return true;
  };
}

/** A text field that also takes the barcode scanner (return ticket, price check, barcodes,
 * searches): a scan puts the decoded code in `onScan` (which replaces the field's value and
 * submits), whatever the characters the Windows layout typed meanwhile. */
export function ScanInput({
  onScan,
  onKeyDown,
  onFocus,
  onBlur,
  ...props
}: ComponentProps<typeof Input> & { onScan: (code: string) => void }) {
  const scanKey = useScanField(onScan);
  // a serial (USB-COM) scanner's codes come to the field that has the focus (HW-020)
  const [focused, setFocused] = useState(false);
  useFieldSerialScans(onScan, focused);
  return (
    <Input
      {...props}
      autoComplete="off"
      onFocus={(event) => {
        setFocused(true);
        onFocus?.(event);
      }}
      onBlur={(event) => {
        setFocused(false);
        onBlur?.(event);
      }}
      onKeyDown={(event) => {
        if (!scanKey(event)) onKeyDown?.(event);
      }}
    />
  );
}
