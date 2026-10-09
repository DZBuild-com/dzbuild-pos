import { dayToMs, isDay } from "@/lib/dates";
import { formatDate, formatDZD, formatNumber } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import type { Cell, ColumnKind, TagDomain } from "@/lib/ipc/types";
import { useLang } from "@/lib/use-language";

/** Kinds shown left-to-right and end-aligned: numbers, amounts, dates, codes. */
const NUMERIC: readonly ColumnKind[] = ["money", "qty", "int", "percent"];
const LTR: readonly ColumnKind[] = [...NUMERIC, "date", "date_time", "code"];

export function isNumericKind(kind: ColumnKind): boolean {
  return NUMERIC.includes(kind);
}

/**
 * A backend table cell as text in `lang` (pos-core `table`: money in centimes, quantities in
 * milli-units, percents in basis points, dates `YYYY-MM-DD`, instants in unix milliseconds; tags
 * through the report's `tagLabels`, keyed `"<domain>.<value>"`). Empty cells give "".
 */
export function formatCell(
  kind: ColumnKind,
  value: Cell,
  lang: Lang,
  tag: TagDomain | null = null,
  tagLabels: Partial<Record<string, string>> = {},
): string {
  if (value === null) return "";
  if (typeof value === "string") {
    if (kind === "date" && isDay(value)) return formatDate(dayToMs(value), lang);
    if (kind === "tag" && tag) return tagLabels[`${tag}.${value}`] ?? value;
    return value;
  }
  switch (kind) {
    case "money":
      return formatDZD(value, lang);
    case "qty":
      return formatNumber(value / 1_000, lang, 3);
    case "percent":
      return `${formatNumber(value / 100, lang, 2)}${lang === "fr" ? "\u00A0%" : "%"}`;
    case "date_time":
      return formatDate(value, lang, { withTime: true });
    case "date":
      return formatDate(value, lang);
    case "int":
      return formatNumber(value, lang);
    default:
      return String(value);
  }
}

/** One report cell formatted by its column kind; numbers, dates and codes stay left-to-right
 * inside Arabic (`<bdi dir="ltr">`). */
export function ReportCell({
  kind,
  value,
  tag = null,
  tagLabels,
}: {
  kind: ColumnKind;
  value: Cell;
  tag?: TagDomain | null;
  tagLabels?: Partial<Record<string, string>>;
}) {
  const lang = useLang();
  const text = formatCell(kind, value, lang, tag, tagLabels);
  if (text === "") return null;
  if (LTR.includes(kind)) return <bdi dir="ltr">{text}</bdi>;
  return <>{text}</>;
}
