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
/** The pause in typing after which a name is searched. */
const SEARCH_DELAY_MS = 150;
const NO_HITS: ProductHit[] = [];

/** Typed digits only (Arabic-Indic ones already made Western): a barcode, a PLU, a scale label. */
function isDigits(text: string): boolean {
  return /^\d+$/.test(text);
}

/** Text searched by name: two characters or more, not a number. */
function searchable(text: string): boolean {
  return text.length >= 2 && !isDigits(text);
}

/** One word with a digit in it (a reference, a voucher, a loyalty card): read as a code when no
 * product name matches it. */
function looksLikeCode(text: string): boolean {
  return /\d/.test(text) && !/\s/.test(text);
}

function startsWith(text: string, start: string): boolean {
  return text.toLocaleLowerCase().startsWith(start.toLocaleLowerCase());
}

interface Found {
  query: string;
  hits: ProductHit[];
}

/** The list found for `found.query` may stay up for `typed` until the search of `typed` answers:
 * more letters typed only narrow it, and an erased letter widens a list that is not empty. */
function standsFor(found: Found, typed: string): boolean {
  return startsWith(typed, found.query) || (found.hits.length > 0 && startsWith(found.query, typed));
}

const optionId = (index: number) => `pos-scan-option-${index}`;

/** Spec §10.5: always focused, 56 px, brand focus ring. Digits and Enter look a code up (barcode,
 * PLU, scale label); a name lists the products found, ↑/↓ move through them and Enter takes the
 * highlighted one (the first by default), Esc drops the text typed. `3*` sets the next quantity;
 * with the field empty, `+`/`-` change the selected line, `Del` voids it, ↑/↓ move the selection.
 *
 * The list only ever shows the results of the text in the field, or of its start while more is
 * typed: never the last search's after a pick. Enter before the results of the text typed are in
 * runs that search at once and takes its first product.
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
  onEmptyEnter,
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
  /** Enter on the empty field (the till's "one method" checkout pays). */
  onEmptyEnter?: () => void;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const lang = useLang();
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
  const typed = normalizeDigits(value.trim());
  // the search runs once typing pauses, or at once when Enter needs its answer
  const debounced = useDebounced(typed, SEARCH_DELAY_MS);
  const [flushed, setFlushed] = useState<string | null>(null);
  const text = flushed === typed ? typed : debounced;
  const results = useProductSearch(
    { text, limit: 8, categoryId: null, includeArchived: false },
    searchable(text),
  );
  // The results of the last search answered, with the text they were found for: while the next
  // search runs, the query holds the previous text's results and cannot say which text that was.
  // An emptied field (a pick, Esc, a code read) forgets them: the next text starts a list afresh.
  const [found, setFound] = useState<Found | null>(null);
  const answer = results.isPlaceholderData ? undefined : results.data;
  const answered = answer !== undefined && searchable(text) && startsWith(typed, text);
  if (answered && (found?.query !== text || found.hits !== answer)) {
    setFound({ query: text, hits: answer });
  } else if (!answered && typed === "" && found !== null) {
    setFound(null);
  }
  const shown = found !== null && searchable(typed) && standsFor(found, typed) ? found : null;
  const hits = shown?.hits ?? NO_HITS;
  /** The list on screen was found for exactly the text in the field. */
  const current = shown !== null && shown.query === typed;
  // the product the cashier moved to (arrows, pointer); the first one otherwise
  const [cursor, setCursor] = useState<string | null>(null);
  const moved = cursor === null ? -1 : hits.findIndex((h) => h.id === cursor);
  const highlight = hits.length === 0 ? -1 : Math.max(0, moved);
  // Enter pressed before the results of the text typed were in: the text it was pressed on
  const pendingEnter = useRef<string | null>(null);
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    if (highlight >= 0) listRef.current?.children.item(highlight)?.scrollIntoView?.({ block: "nearest" });
  }, [highlight, hits]);

  // the answer Enter waits for: its first product, else the text read as a code when it is one
  useEffect(() => {
    const waiting = pendingEnter.current;
    if (waiting === null) return;
    if (waiting !== typed) {
      pendingEnter.current = null;
      return;
    }
    if (found?.query !== waiting) return;
    pendingEnter.current = null;
    const first = found.hits[0];
    if (first) onPick(first);
    else if (looksLikeCode(waiting)) onCode(waiting);
  }, [typed, found, onPick, onCode]);

  const pick = (hit: ProductHit) => {
    setCursor(null);
    pendingEnter.current = null;
    onPick(hit);
  };

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
      if (typed === "") {
        onEmptyEnter?.();
        return;
      }
      if (!searchable(typed)) {
        if (isDigits(typed)) onCode(typed);
        return;
      }
      const hit = hits[highlight];
      if (hit && (current || moved >= 0)) {
        pick(hit);
      } else if (current) {
        // no product is called that: the text stays, and is read as a code only if it looks like one
        if (looksLikeCode(typed)) onCode(typed);
      } else {
        pendingEnter.current = typed;
        setFlushed(typed);
      }
    } else if (event.key === "Escape" && value !== "") {
      // the text typed (and its list) goes; the menu's Esc is for an empty field
      event.preventDefault();
      event.stopPropagation();
      pendingEnter.current = null;
      setCursor(null);
      onValueChange("");
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
      const next = hits[(highlight + delta + hits.length) % hits.length];
      if (next) setCursor(next.id);
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
            aria-autocomplete="list"
            aria-expanded={hits.length > 0}
            aria-controls="pos-scan-results"
            aria-activedescendant={highlight >= 0 ? optionId(highlight) : undefined}
            value={value}
            onChange={(e) => {
              pendingEnter.current = null;
              setCursor(null);
              onValueChange(e.target.value);
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
          ref={listRef}
          id="pos-scan-results"
          role="listbox"
          aria-label={t("pos:scan.results")}
          className="absolute inset-x-0 top-full z-30 mt-1 max-h-[min(24rem,calc(100dvh-var(--osk-inset,0px)-12rem))] overflow-y-auto rounded-card border border-line bg-surface shadow-overlay"
        >
          {hits.map((hit, i) => (
            <li
              key={hit.id}
              id={optionId(i)}
              role="option"
              aria-selected={i === highlight}
              // the field keeps the focus: the scanner and the keyboard still type there
              onMouseDown={(e) => {
                e.preventDefault();
                pick(hit);
              }}
              onPointerMove={() => {
                if (i !== highlight) setCursor(hit.id);
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
      ) : shown !== null ? (
        <p
          role="status"
          className="absolute inset-x-0 top-full z-30 mt-1 rounded-card border border-line bg-surface px-4 py-3 text-ink-2 shadow-overlay"
        >
          {t(looksLikeCode(typed) ? "pos:scan.noMatchCode" : "pos:scan.noMatch", { text: value.trim() })}
        </p>
      ) : null}
    </div>
  );
}
