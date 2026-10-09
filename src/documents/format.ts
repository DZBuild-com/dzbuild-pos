import { formatDZD, formatNumber } from "@/lib/format";
import type { Lang } from "@/lib/i18n";

/** Amounts on paper: integer centimes → "23 449,00 DA" in the document's language. */
export const docMoney = (centimes: number, lang: Lang) => formatDZD(centimes, lang);

/** Quantities on paper: milli-units → "10", "2,5", "0,125". */
export const docQty = (milli: number, lang: Lang) => formatNumber(milli / 1000, lang, 3);

/** A rate in basis points → "19 %". */
export const docRate = (bp: number, lang: Lang) => `${formatNumber(bp / 100, lang, 2)}\u00A0%`;

/** A day `YYYY-MM-DD` → "28/09/2026" (Western digits in every language); any other text as is. */
export const docDay = (day: string) => {
  const parts = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
  return parts ? `${parts[3]}/${parts[2]}/${parts[1]}` : day;
};
