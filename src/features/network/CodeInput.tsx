import { type ComponentProps, useLayoutEffect, useRef } from "react";
import { digitFromKey } from "@/components/pin-pad";
import { Input } from "@/components/ui/input";
import { toAsciiDigits } from "@/lib/digits";
import { cn } from "@/lib/utils";

const LENGTH = 6;

/** The 6-digit pairing code, visible (it is read off the main PC's screen); AZERTY and Arabic
 * top-row keys type digits at the caret, as in `PinInput`. */
export function CodeInput({
  value,
  onChange,
  className,
  ...props
}: Omit<ComponentProps<"input">, "value" | "onChange" | "type"> & {
  value: string;
  onChange: (value: string) => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  // Where the caret goes once a digit typed by physical key is rendered (as `PinInput`).
  const caretRef = useRef<number | null>(null);
  useLayoutEffect(() => {
    const caret = caretRef.current;
    caretRef.current = null;
    if (caret !== null) inputRef.current?.setSelectionRange(caret, caret);
  }, [value]);
  return (
    <Input
      {...props}
      ref={inputRef}
      type="text"
      inputMode="numeric"
      autoComplete="off"
      dir="ltr"
      maxLength={LENGTH}
      value={value}
      className={cn("font-mono text-2xl tracking-[0.3em]", className)}
      onKeyDown={(event) => {
        const digit = digitFromKey(event);
        if (!digit || event.key === digit) return;
        event.preventDefault();
        const input = event.currentTarget;
        const start = input.selectionStart ?? value.length;
        const end = input.selectionEnd ?? start;
        if (start === end && value.length >= LENGTH) return;
        caretRef.current = start + 1;
        onChange((value.slice(0, start) + digit + value.slice(end)).slice(0, LENGTH));
      }}
      onChange={(event) => onChange(toAsciiDigits(event.target.value).replace(/\D/g, "").slice(0, LENGTH))}
    />
  );
}
