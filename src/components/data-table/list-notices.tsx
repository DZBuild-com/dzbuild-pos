import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { formatNumber } from "@/lib/format";
import { useLang } from "@/lib/use-language";

/** A list asked for `limit + 1` rows: the first `limit` shown, and whether more exist. */
export function capped<T>(rows: readonly T[] | undefined, limit: number): { rows: T[]; truncated: boolean } {
  const all = rows ?? [];
  return { rows: all.slice(0, limit), truncated: all.length > limit };
}

/** Says a list stopped at its limit (like the reports' "first N rows"), and that its sorting is
 * off: sorting a cut list would rank only what was loaded. */
export function TruncatedNotice({ rows }: { rows: number }) {
  const { t } = useTranslation();
  const lang = useLang();
  return (
    <p role="status" className="rounded-control border border-warn bg-warn-soft p-3 text-warn">
      {t("tables:truncated", { rows: formatNumber(rows, lang) })} {t("tables:truncatedSort")}
    </p>
  );
}

/** The next page of a history read newest first. */
export function LoadMore({ onClick, pending }: { onClick: () => void; pending: boolean }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col items-center gap-2 pt-3">
      <p className="text-sm text-ink-2">{t("tables:olderRows")}</p>
      <Button variant="outline" disabled={pending} onClick={onClick}>
        {t("tables:loadMore")}
      </Button>
    </div>
  );
}
