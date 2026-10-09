import type { Lang } from "@/lib/i18n";

// Invisible characters are always written as escapes, never typed:
// U+00A0 NO-BREAK SPACE (before the currency symbol), U+202F NARROW NO-BREAK SPACE (thousands, fr/ar).
const NBSP = "\u00A0";
const NNBSP = "\u202F";
const LOCALES: Record<Lang, string> = { ar: "ar-DZ", fr: "fr-DZ", en: "en-GB" };

function group(digits: string, separator: string): string {
  return digits.replace(/\B(?=(\d{3})+(?!\d))/g, separator);
}

/** Integer centimes → "4 500,00 DA" (fr), "4,500.00 DA" (en), "4 500,00 دج" (ar). No floats. */
export function formatDZD(centimes: number, lang: Lang, options: { hideCentimes?: boolean } = {}): string {
  if (!Number.isSafeInteger(centimes)) {
    throw new RangeError(`amount must be an integer number of centimes, got ${centimes}`);
  }
  const abs = Math.abs(centimes);
  const separator = lang === "en" ? "," : NNBSP;
  const decimal = lang === "en" ? "." : ",";
  const symbol = lang === "ar" ? "دج" : "DA";
  const number = options.hideCentimes
    ? group(String(Math.floor((abs + 50) / 100)), separator)
    : `${group(String(Math.floor(abs / 100)), separator)}${decimal}${String(abs % 100).padStart(2, "0")}`;
  const sign = centimes < 0 && number !== "0" ? "-" : "";
  return `${sign}${number}${NBSP}${symbol}`;
}

/** Splits "1.000.000,50" / "1,000,000.50" / "4500" into whole and fraction parts. */
function splitDecimal(input: string): { whole: string; fraction: string } | null {
  const cleaned = input.replace(/[\s\u00A0\u202F']/g, "").replace(/(DA|دج)$/iu, "");
  if (cleaned === "") return null;
  const last = Math.max(cleaned.lastIndexOf(","), cleaned.lastIndexOf("."));
  const isDecimal = last >= 0 && cleaned.length - last - 1 <= 2;
  const whole = (isDecimal ? cleaned.slice(0, last) : cleaned).replace(/[.,]/g, "");
  const fraction = isDecimal ? cleaned.slice(last + 1) : "";
  if (!/^\d+$/.test(whole) || !/^\d{0,2}$/.test(fraction)) return null;
  return { whole, fraction };
}

/** Typed amount → integer centimes, or null. */
export function parseDZD(input: string): number | null {
  const parts = splitDecimal(input);
  if (!parts) return null;
  const centimes = Number(parts.whole) * 100 + Number(parts.fraction.padEnd(2, "0"));
  return Number.isSafeInteger(centimes) ? centimes : null;
}

/** "5,5" → 550 basis points; null outside 0–100 %. */
export function parsePercentBp(input: string): number | null {
  const parts = splitDecimal(input.replace(/%$/, ""));
  if (!parts) return null;
  const bp = Number(parts.whole) * 100 + Number(parts.fraction.padEnd(2, "0"));
  return bp <= 10_000 ? bp : null;
}

/** Integer centimes → what an amount input shows: "1000000,50" (fr/ar), "1000000.50" (en). No grouping,
 * so `parseDZD` reads it back exactly. Components call this instead of building amount text. */
export function formatAmountInput(centimes: number, lang: Lang): string {
  if (!Number.isSafeInteger(centimes)) {
    throw new RangeError(`amount must be an integer number of centimes, got ${centimes}`);
  }
  const abs = Math.abs(centimes);
  const sign = centimes < 0 ? "-" : "";
  return `${sign}${Math.floor(abs / 100)}${lang === "en" ? "." : ","}${String(abs % 100).padStart(2, "0")}`;
}

/** 550 → "5,5" (fr/ar) or "5.5" (en). */
export function formatPercentInput(bp: number, lang: Lang): string {
  const whole = Math.trunc(bp / 100);
  const fraction = bp % 100;
  if (fraction === 0) return String(whole);
  const digits = String(fraction).padStart(2, "0").replace(/0$/, "");
  return `${whole}${lang === "en" ? "." : ","}${digits}`;
}

// Intl's Arabic output carries bidi marks (U+200E LRM, U+200F RLM, U+061C ALM) meant for an RTL
// run. Callers show dates and numbers in `<bdi dir="ltr">`, where those marks reorder the
// characters ("28/09/2026" came out scrambled), so they are removed.
const BIDI_MARKS = /[\u200E\u200F\u061C]/g;
// The Arabic comma (U+060C) Intl puts between the date and the time.
const ARABIC_COMMA_SPACE = /،\s*/g;

/** Gregorian date in Africa/Algiers with Western digits, free of bidi marks: "28/09/2026 13:00" in
 * every language, safe inside `<bdi dir="ltr">`. */
export function formatDate(ms: number, lang: Lang, options: { withTime?: boolean } = {}): string {
  const format: Intl.DateTimeFormatOptions = {
    timeZone: "Africa/Algiers",
    calendar: "gregory",
    numberingSystem: "latn",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  };
  if (options.withTime) {
    format.hour = "2-digit";
    format.minute = "2-digit";
    format.hourCycle = "h23";
  }
  return new Intl.DateTimeFormat(LOCALES[lang], format)
    .format(ms)
    .replace(BIDI_MARKS, "")
    .replace(ARABIC_COMMA_SPACE, " ");
}

/** Western digits, no bidi marks (see `formatDate`). */
export function formatNumber(value: number, lang: Lang, maximumFractionDigits = 0): string {
  return new Intl.NumberFormat(LOCALES[lang], { numberingSystem: "latn", maximumFractionDigits })
    .format(value)
    .replace(BIDI_MARKS, "");
}
