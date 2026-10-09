import { type ComponentProps, type KeyboardEvent, useRef } from "react";
import { digitFromKey } from "@/components/pin-pad";
import { Input } from "@/components/ui/input";
import { normalizeDigits } from "@/lib/numeric";
import { SCAN_MAX_GAP_MS, burstGuardOn } from "@/lib/scanner";

type Kind = "amount" | "qty" | "integer";

function clean(text: string, kind: Kind): string {
  const normal = normalizeDigits(text).replace(/\./g, ",");
  if (kind === "integer") return normal.replace(/\D/g, "");
  const [whole = "", ...rest] = normal.replace(/[^\d,]/g, "").split(",");
  return rest.length === 0 ? whole : `${whole},${rest.join("").slice(0, kind === "amount" ? 2 : 3)}`;
}

/** HW-022: keys arriving faster than a person types are a scanner's. They are dropped and the
 * value from before the burst is put back; the Enter ending the burst is swallowed too, so a scan
 * never validates the form the field is in. Returns the keydown handler; `true` means handled. */
function useBurstGuard(
  value: string,
  onValueChange: (value: string) => void,
): (event: KeyboardEvent<HTMLInputElement>) => boolean {
  const last = useRef<{ at: number; before: string } | null>(null);
  return (event) => {
    const previous = last.current;
    const inBurst = burstGuardOn() && previous !== null && event.timeStamp - previous.at < SCAN_MAX_GAP_MS;
    if (event.key === "Enter" && inBurst) {
      event.preventDefault();
      event.stopPropagation();
      last.current = null;
      onValueChange(previous.before);
      return true;
    }
    const printable = event.key.length === 1;
    if (printable && inBurst) {
      event.preventDefault();
      last.current = { at: event.timeStamp, before: previous.before };
      onValueChange(previous.before);
      return true;
    }
    if (printable) last.current = { at: event.timeStamp, before: value };
    return false;
  };
}

/** Amounts, quantities and counts: always LTR (spec §10.4), AZERTY top row and numeric keypad type
 * digits (UX-031), Arabic-Indic digits are converted (UX-014), and a scanner burst never lands here:
 * keys arriving faster than a person types are dropped and the value is restored (HW-022). */
export function NumericInput({
  value,
  onValueChange,
  kind = "amount",
  onKeyDown,
  ...props
}: Omit<ComponentProps<"input">, "value" | "onChange" | "type"> & {
  value: string;
  onValueChange: (value: string) => void;
  kind?: Kind;
}) {
  const guard = useBurstGuard(value, onValueChange);
  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (guard(event)) return;
    const digit = digitFromKey(event);
    if (digit && event.key !== digit) {
      event.preventDefault();
      onValueChange(clean(value + digit, kind));
    }
    onKeyDown?.(event);
  };
  return (
    <Input
      {...props}
      type="text"
      inputMode={kind === "integer" ? "numeric" : "decimal"}
      autoComplete="off"
      dir="ltr"
      value={value}
      onKeyDown={handleKeyDown}
      onChange={(event) => onValueChange(clean(event.target.value, kind))}
      className={props.className ? `tabular-nums ${props.className}` : "tabular-nums"}
    />
  );
}

/** A free-text field of a form a scan must never reach (a card or cheque reference): the same
 * burst guard as [`NumericInput`], without the digit rules. */
export function GuardedInput({
  value,
  onValueChange,
  onKeyDown,
  ...props
}: Omit<ComponentProps<"input">, "value" | "onChange"> & {
  value: string;
  onValueChange: (value: string) => void;
}) {
  const guard = useBurstGuard(value, onValueChange);
  return (
    <Input
      {...props}
      autoComplete="off"
      value={value}
      onKeyDown={(event) => {
        if (!guard(event)) onKeyDown?.(event);
      }}
      onChange={(event) => onValueChange(event.target.value)}
    />
  );
}
