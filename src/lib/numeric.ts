import { parseDZD } from "@/lib/format";
import type { Lang } from "@/lib/i18n";

/** Arabic-Indic (٠-٩) and Persian (۰-۹) digits → 0-9; ٫ → "," and ٬ dropped (UX-014: both accepted). */
export function normalizeDigits(input: string): string {
  return input
    .replace(/[٠-٩]/g, (d) => String((d.codePointAt(0) ?? 0x0660) - 0x0660))
    .replace(/[۰-۹]/g, (d) => String((d.codePointAt(0) ?? 0x06f0) - 0x06f0))
    .replace(/٫/g, ",")
    .replace(/٬/g, "");
}

/** A typed amount → integer centimes, or null. */
export function parseAmount(input: string): number | null {
  return parseDZD(normalizeDigits(input));
}

/** A typed quantity → milli-units (> 0) within `decimals` (0 pieces, 3 kg/m), or null. */
export function parseQty(input: string, decimals: number): number | null {
  // U+00A0 NO-BREAK SPACE and U+202F NARROW NO-BREAK SPACE (thousands separators) are dropped.
  const text = normalizeDigits(input)
    .replace(/[\s\u00A0\u202F]/g, "")
    .replace(",", ".");
  if (!/^\d+(\.\d{0,3})?$/.test(text)) return null;
  const [whole = "0", fraction = ""] = text.split(".");
  if (fraction.length > decimals) return null;
  const milli = Number(whole) * 1_000 + Number(fraction.padEnd(3, "0"));
  return Number.isSafeInteger(milli) && milli > 0 ? milli : null;
}

/** [`parseQty`] where zero is a quantity too (a count that found nothing, no minimum). */
export function parseQtyOrZero(input: string, decimals: number): number | null {
  const text = normalizeDigits(input).replace(/[\s\u00A0\u202F]/g, "");
  return /^0+([.,]0*)?$/.test(text) ? 0 : parseQty(input, decimals);
}

/** 2000 → "2", 1250 → "1,25" (fr, ar) / "1.25" (en). */
export function formatQty(milli: number, lang: Lang): string {
  const sign = milli < 0 ? "-" : "";
  const abs = Math.abs(milli);
  const whole = Math.floor(abs / 1_000);
  const fraction = abs % 1_000;
  if (fraction === 0) return `${sign}${whole}`;
  const digits = String(fraction).padStart(3, "0").replace(/0+$/, "");
  return `${sign}${whole}${lang === "en" ? "." : ","}${digits}`;
}

/** 1900 → "19 %" (fr, U+00A0 NO-BREAK SPACE) / "19%" (en, ar). */
export function formatRate(bp: number, lang: Lang): string {
  const whole = Math.trunc(bp / 100);
  const fraction = Math.abs(bp % 100);
  const number =
    fraction === 0
      ? String(whole)
      : `${whole}${lang === "en" ? "." : ","}${String(fraction).padStart(2, "0").replace(/0$/, "")}`;
  return lang === "fr" ? `${number}\u00A0%` : `${number}%`;
}
