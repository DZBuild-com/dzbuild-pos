import { Delete, X } from "lucide-react";
import {
  type ComponentProps,
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useRef,
} from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

export const PIN_MIN = 4;
export const PIN_MAX = 8;

const DIGIT_CODES: Record<string, string> = {
  Digit0: "0",
  Digit1: "1",
  Digit2: "2",
  Digit3: "3",
  Digit4: "4",
  Digit5: "5",
  Digit6: "6",
  Digit7: "7",
  Digit8: "8",
  Digit9: "9",
  Numpad0: "0",
  Numpad1: "1",
  Numpad2: "2",
  Numpad3: "3",
  Numpad4: "4",
  Numpad5: "5",
  Numpad6: "6",
  Numpad7: "7",
  Numpad8: "8",
  Numpad9: "9",
};

/** Digit from the physical key, so AZERTY (&é"'(…) and Arabic layouts still type digits. */
export function digitFromKey(event: Pick<KeyboardEvent, "code" | "key">): string | null {
  const byCode = DIGIT_CODES[event.code];
  if (byCode) return byCode;
  return /^[0-9]$/.test(event.key) ? event.key : null;
}

function PadKey({
  onPress,
  disabled,
  label,
  children,
}: {
  onPress: () => void;
  disabled?: boolean | undefined;
  label?: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      tabIndex={-1}
      aria-label={label}
      disabled={disabled}
      onClick={onPress}
      className="grid size-16 place-items-center rounded-control bg-surface-2 text-2xl font-bold hover:bg-line disabled:opacity-50"
    >
      {children}
    </button>
  );
}

/** 64 px keys, always left-to-right; physical keyboard and numeric keypad both work (spec §10.5). */
export function PinPad({
  value,
  onChange,
  onSubmit,
  onCancel,
  submitLabel,
  disabled,
}: {
  value: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  onCancel?: () => void;
  submitLabel: string;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const padRef = useRef<HTMLDivElement>(null);
  const isEmpty = value === "";
  // React 19 only honours the autofocus prop on form controls, never on a <div>, so the pad
  // focuses itself: on mount and each time the PIN becomes empty (clear key, wrong PIN cleared
  // by the parent). The shop keyboard then types without a click first (Review Focus #4).
  useEffect(() => {
    if (isEmpty) padRef.current?.focus();
  }, [isEmpty]);
  const push = (digit: string) => {
    if (!disabled && value.length < PIN_MAX) onChange(value + digit);
  };
  const erase = () => onChange(value.slice(0, -1));
  const submit = () => {
    if (!disabled && value.length >= PIN_MIN) onSubmit();
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const digit = digitFromKey(event);
    if (digit) {
      event.preventDefault();
      push(digit);
    } else if (event.key === "Backspace") {
      event.preventDefault();
      erase();
    } else if (event.key === "Enter") {
      event.preventDefault();
      submit();
    } else if (event.key === "Escape" && onCancel) {
      event.preventDefault();
      onCancel();
    }
  };
  return (
    <div
      ref={padRef}
      role="group"
      aria-label={t("auth:pinPad.label")}
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="flex flex-col items-center gap-4 rounded-card p-2 outline-none focus-visible:ring-3 focus-visible:ring-focus"
    >
      <div className="flex h-8 items-center gap-3" dir="ltr" aria-live="polite">
        <span className="sr-only">{t("auth:pinPad.digits", { n: value.length })}</span>
        {Array.from({ length: Math.max(PIN_MIN, value.length) }, (_, index) => (
          <span
            key={index}
            aria-hidden
            className={cn("size-4 rounded-full border-2 border-ink", index < value.length && "bg-ink")}
          />
        ))}
      </div>
      <div data-testid="pin-keys" className="grid grid-cols-3 gap-3" dir="ltr">
        {["1", "2", "3", "4", "5", "6", "7", "8", "9"].map((digit) => (
          <PadKey key={digit} onPress={() => push(digit)} disabled={disabled}>
            {digit}
          </PadKey>
        ))}
        <PadKey onPress={() => onChange("")} disabled={disabled} label={t("auth:pinPad.clear")}>
          <X aria-hidden />
        </PadKey>
        <PadKey onPress={() => push("0")} disabled={disabled}>
          0
        </PadKey>
        <PadKey onPress={erase} disabled={disabled} label={t("auth:pinPad.erase")}>
          <Delete aria-hidden />
        </PadKey>
      </div>
      <Button
        variant="primary"
        size="lg"
        className="w-full"
        onClick={submit}
        disabled={disabled || value.length < PIN_MIN}
        shortcut="Enter"
        shortcutLabel={t("common:keys.enter")}
      >
        {submitLabel}
      </Button>
    </div>
  );
}

/** Masked numeric input for forms; AZERTY/Arabic top-row keys (and the keypad with NumLock off)
 * still type digits, at the caret or over the selection like any typed character. */
export function PinInput({
  value,
  onChange,
  ...props
}: Omit<ComponentProps<"input">, "value" | "onChange" | "type"> & {
  value: string;
  onChange: (value: string) => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  // Where the caret goes once a digit typed by physical key is rendered: React moves it to the
  // end when it writes the new value.
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
      type="password"
      inputMode="numeric"
      autoComplete="off"
      dir="ltr"
      maxLength={PIN_MAX}
      value={value}
      onKeyDown={(event) => {
        const digit = digitFromKey(event);
        if (!digit || event.key === digit) return;
        event.preventDefault();
        const input = event.currentTarget;
        const start = input.selectionStart ?? value.length;
        const end = input.selectionEnd ?? start;
        if (start === end && value.length >= PIN_MAX) return;
        caretRef.current = start + 1;
        onChange((value.slice(0, start) + digit + value.slice(end)).slice(0, PIN_MAX));
      }}
      onChange={(event) => onChange(event.target.value.replace(/\D/g, "").slice(0, PIN_MAX))}
    />
  );
}
