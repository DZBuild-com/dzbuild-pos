import { ScanLine, Search } from "lucide-react";
import { type KeyboardEvent, type RefObject, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { useProductSearch } from "@/lib/ipc/products";
import type { ProductHit } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty, normalizeDigits, parseQty } from "@/lib/numeric";
import { SCAN_MAX_GAP_MS } from "@/lib/scanner";
import { useDebounced } from "@/lib/use-debounced";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** How long `+`, `-` and `*` wait for a scanner's next key before they act: past the scanner's
 * largest gap, with room for a busy event loop, and too short for a person to notice. */
const HOLD_MS = SCAN_MAX_GAP_MS * 2;

/** Spec §10.5: always focused, 56 px, brand focus ring. Enter looks a code up (barcode, reference,
 * PLU) or takes the highlighted search result; `3*` sets the next quantity; with the field empty,
 * `+`/`-` change the selected line, `Del` voids it, ↑/↓ move the selection.
 *
 * `+`, `-` and `*` may be the first key of a scanner burst: a French AZERTY Windows turns a US
 * scanner's "6" into "-", and every Algerian EAN-13 starts with 613. They act only when no key
 * follows within the scanner's gap (HW-021/022); a key that does cancels them, the window-level
 * scanner reads the code, and the lookup clears the field. A held-down key (auto-repeat) is a
 * person's: it acts at once. */
export function ScanField({
  inputRef,
  value,
  onValueChange,
  multiplier,
  onCode,
  onPick,
  onMultiplier,
  onBump,
  onVoid,
  onMove,
  disabled,
}: {
  inputRef: RefObject<HTMLInputElement | null>;
  value: string;
  onValueChange: (value: string) => void;
  multiplier: number | null;
  onCode: (code: string) => void;
  onPick: (hit: ProductHit) => void;
  onMultiplier: (qty: number) => void;
  onBump: (direction: 1 | -1) => void;
  onVoid: () => void;
  onMove: (delta: 1 | -1) => void;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const [highlight, setHighlight] = useState(-1);
  // "Rechercher par nom": the on-screen keyboard types in this field until it is left, Enter or
  // Escape (finding 21); the rest of the time the scanner has it to itself.
  const [typing, setTyping] = useState(false);
  const wasTyping = useRef(false);
  const refocusing = useRef(false);
  useEffect(() => {
    const input = inputRef.current;
    const changed = wasTyping.current !== typing;
    wasTyping.current = typing;
    if (!input || !changed) return;
    // The keyboard reads the field when it gets the focus: give it the focus again.
    if (typing || document.activeElement === input) {
      refocusing.current = true;
      input.blur();
      input.focus();
      refocusing.current = false;
    }
  }, [typing, inputRef]);
  const text = useDebounced(value.trim(), 150);
  const searching = text.length >= 2 && !/^\d+$/.test(text);
  const results = useProductSearch({ text, limit: 8, categoryId: null, includeArchived: false }, searching);
  const hits = searching && value.trim() !== "" ? (results.data ?? []) : [];
  const held = useRef<{ at: number; timer: number; run: () => void } | null>(null);
  useEffect(
    () => () => {
      if (held.current) window.clearTimeout(held.current.timer);
    },
    [],
  );

  /** The key held back, if any: dropped when this key is the scanner's next one, else run now. */
  const settle = (now: number) => {
    const pending = held.current;
    if (!pending) return;
    held.current = null;
    window.clearTimeout(pending.timer);
    if (now - pending.at > SCAN_MAX_GAP_MS) pending.run();
  };

  const holdBack = (event: KeyboardEvent<HTMLInputElement>, run: () => void) => {
    event.preventDefault();
    if (event.repeat) {
      run();
      return;
    }
    const timer = window.setTimeout(() => {
      held.current = null;
      run();
    }, HOLD_MS);
    held.current = { at: event.timeStamp, timer, run };
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    settle(event.timeStamp);
    const empty = value === "";
    if (event.key === "Enter" || event.key === "Escape") setTyping(false);
    if (event.key === "Enter") {
      event.preventDefault();
      const hit = hits[highlight];
      if (hit) {
        onPick(hit);
        setHighlight(-1);
      } else if (value.trim() !== "") {
        onCode(normalizeDigits(value.trim()));
      }
    } else if (event.key === "*" || event.code === "NumpadMultiply") {
      const qty = parseQty(value, 3);
      if (qty !== null) {
        holdBack(event, () => {
          onMultiplier(qty);
          onValueChange("");
        });
      }
    } else if (empty && (event.key === "+" || event.code === "NumpadAdd")) {
      holdBack(event, () => onBump(1));
    } else if (empty && (event.key === "-" || event.code === "NumpadSubtract")) {
      holdBack(event, () => onBump(-1));
    } else if (empty && event.key === "Delete") {
      event.preventDefault();
      onVoid();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const delta = event.key === "ArrowDown" ? 1 : -1;
      if (hits.length > 0) setHighlight((h) => Math.max(-1, Math.min(hits.length - 1, h + delta)));
      else onMove(delta);
    }
  };

  return (
    <div className="relative flex flex-col gap-1">
      <label htmlFor="pos-scan" className="text-sm font-semibold text-ink-2">
        {t("pos:scan.label")}
      </label>
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <ScanLine
            aria-hidden
            className="pointer-events-none absolute start-3 top-1/2 size-6 -translate-y-1/2 text-ink-2"
          />
          <input
            ref={inputRef}
            id="pos-scan"
            dir="auto"
            autoFocus
            autoComplete="off"
            // The scanner types here and the field always has the focus: the on-screen keyboard
            // would never leave the sale screen, except while "Rechercher par nom" is on.
            data-osk={typing ? undefined : "off"}
            onBlur={() => {
              if (!refocusing.current) setTyping(false);
            }}
            disabled={disabled}
            role="combobox"
            aria-expanded={hits.length > 0}
            aria-controls="pos-scan-results"
            value={value}
            onChange={(e) => {
              onValueChange(e.target.value);
              setHighlight(-1);
            }}
            onKeyDown={onKeyDown}
            className="h-14 w-full rounded-control border-2 border-line bg-surface ps-12 pe-3 text-xl focus:border-brand focus:ring-3 focus:ring-brand focus:outline-none"
          />
        </div>
        <button
          type="button"
          disabled={disabled}
          aria-pressed={typing}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => setTyping(true)}
          className="flex h-14 shrink-0 items-center gap-2 rounded-control border-2 border-line bg-surface px-4 font-semibold text-ink hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-brand focus-visible:outline-none disabled:opacity-50 aria-pressed:border-brand aria-pressed:bg-brand-soft"
        >
          <Search aria-hidden className="size-5" />
          {t("pos:scan.typeName")}
        </button>
        {multiplier !== null ? (
          <span className="rounded-full bg-brand px-3 py-2 text-lg font-bold text-brand-ink">
            {t("pos:scan.times", { qty: formatQty(multiplier, lang) })}
          </span>
        ) : null}
      </div>
      {hits.length > 0 ? (
        <ul
          id="pos-scan-results"
          role="listbox"
          aria-label={t("pos:scan.results")}
          className="absolute inset-x-0 top-full z-30 mt-1 max-h-[min(24rem,calc(100dvh-var(--osk-inset,0px)-12rem))] overflow-y-auto rounded-card border border-line bg-surface shadow-overlay"
        >
          {hits.map((hit, i) => (
            <li
              key={hit.id}
              role="option"
              aria-selected={i === highlight}
              onMouseDown={(e) => {
                e.preventDefault();
                onPick(hit);
              }}
              className={cn(
                "flex min-h-14 cursor-pointer items-center justify-between gap-3 px-4",
                i === highlight && "bg-brand-soft",
              )}
            >
              <span className="font-semibold">{localizedName(hit, lang)}</span>
              <Amount value={hit.sellPrice} className="font-bold" />
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
