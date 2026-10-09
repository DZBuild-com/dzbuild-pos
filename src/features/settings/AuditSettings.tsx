import { ShieldCheck } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { formatDate } from "@/lib/format";
import { useAuditHistory, useAuditLog, useVerifyChain } from "@/lib/ipc/audit";
import { useVerifyClosures } from "@/lib/ipc/cash";
import { useVerifyReceipts } from "@/lib/ipc/credit";
import { useVerifyDocuments } from "@/lib/ipc/documents";
import { useVerifyJournals } from "@/lib/ipc/lan";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** AUD-001/009: families of actions a manager looks for; `null` = the whole journal. */
const FILTERS = [
  null,
  "sale.",
  "return.",
  "cash.",
  "drawer.",
  "document.",
  "product.",
  "stock.",
  "auth.",
] as const;

export function AuditSettings() {
  const { t, i18n } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [family, setFamily] = useState<(typeof FILTERS)[number]>(null);
  const all = useAuditLog();
  const filtered = useAuditHistory(
    { entityType: null, entityId: null, actionPrefix: family },
    family !== null,
  );
  const log = family === null ? all : filtered;
  const verify = useVerifyChain();
  const documents = useVerifyDocuments();
  const closures = useVerifyClosures();
  const receipts = useVerifyReceipts();
  const journals = useVerifyJournals();
  const rows = log.data?.pages.flatMap((page) => page.rows) ?? [];
  const actionLabel = (action: string) => {
    const key = `settings:audit.actions.${action}`;
    return i18n.exists(key) ? t(key) : action;
  };
  return (
    <div className="flex flex-col gap-6">
      <Card>
        <p className="text-ink-2">{t("settings:audit.intro")}</p>
        <div className="flex flex-wrap gap-2">
          <Button variant="primary" disabled={verify.isPending} onClick={() => verify.mutate()}>
            <ShieldCheck aria-hidden />
            {t("settings:audit.verify")}
          </Button>
          <Button
            variant="outline"
            disabled={documents.isPending || closures.isPending || receipts.isPending || journals.isPending}
            onClick={() => {
              documents.mutate();
              closures.mutate();
              receipts.mutate();
              journals.mutate();
            }}
          >
            {t("settings:integrity.verify")}
          </Button>
        </div>
        {verify.data ? (
          verify.data.firstBrokenSeq === null ? (
            <p role="status" className="rounded-control bg-ok-soft p-3 font-semibold text-ok">
              {t("settings:audit.intact", { n: verify.data.rows })}
            </p>
          ) : (
            <p role="alert" className="rounded-control bg-danger-soft p-3 font-semibold text-danger">
              {t("settings:audit.broken", { seq: verify.data.firstBrokenSeq })}
            </p>
          )
        ) : null}
        {documents.data?.map((chain) =>
          chain.firstBrokenSeq === null ? (
            <p
              key={`${chain.docType}-${chain.series}`}
              role="status"
              className="rounded-control bg-ok-soft p-3 font-semibold text-ok"
            >
              {t("settings:integrity.chainIntact", {
                type: t(`documents:types.${chain.docType}`),
                series: chain.series,
                n: chain.rows,
              })}
            </p>
          ) : (
            <p
              key={`${chain.docType}-${chain.series}`}
              role="alert"
              className="rounded-control bg-danger-soft p-3 font-semibold text-danger"
            >
              {t("settings:integrity.chainBroken", {
                type: t(`documents:types.${chain.docType}`),
                series: chain.series,
                number: chain.firstBrokenNumber ?? chain.firstBrokenSeq,
              })}
            </p>
          ),
        )}
        {closures.data ? (
          closures.data.firstBrokenZ === null ? (
            <p role="status" className="rounded-control bg-ok-soft p-3 font-semibold text-ok">
              {t("settings:integrity.closuresIntact", { n: closures.data.rows })}
            </p>
          ) : (
            <p role="alert" className="rounded-control bg-danger-soft p-3 font-semibold text-danger">
              {t("settings:integrity.closuresBroken", { z: closures.data.firstBrokenZ })}
            </p>
          )
        ) : null}
        {receipts.data ? (
          receipts.data.firstBrokenNumber === null ? (
            <p role="status" className="rounded-control bg-ok-soft p-3 font-semibold text-ok">
              {t("settings:integrity.receiptsIntact", { n: receipts.data.rows })}
            </p>
          ) : (
            <p role="alert" className="rounded-control bg-danger-soft p-3 font-semibold text-danger">
              {t("settings:integrity.receiptsBroken", { number: receipts.data.firstBrokenNumber })}
            </p>
          )
        ) : null}
        {journals.data?.tills.map((till) =>
          till.report.broken === 0 ? (
            <p
              key={till.terminalId}
              role="status"
              className="rounded-control bg-ok-soft p-3 font-semibold text-ok"
            >
              {t("settings:integrity.tillJournalIntact", { code: till.terminalCode, n: till.report.rows })}
            </p>
          ) : (
            <p
              key={till.terminalId}
              role="alert"
              className="rounded-control bg-danger-soft p-3 font-semibold text-danger"
            >
              {t("settings:integrity.tillJournalBroken", {
                code: till.terminalCode,
                seq: till.report.firstBrokenSeq ?? 0,
              })}
            </p>
          ),
        )}
        {journals.data && journals.data.tills.length > 0 && journals.data.mergeFirstBroken !== null ? (
          <p role="alert" className="rounded-control bg-danger-soft p-3 font-semibold text-danger">
            {t("settings:integrity.tillMergeBroken")}
          </p>
        ) : null}
        {[verify.error, documents.error, closures.error, receipts.error, journals.error]
          .filter(Boolean)
          .map((e, i) => (
            <p key={i} role="alert" className="text-danger">
              {errorText(e)}
            </p>
          ))}
      </Card>
      <Card>
        <div role="radiogroup" aria-label={t("settings:audit.filter")} className="flex flex-wrap gap-2">
          {FILTERS.map((f) => (
            <button
              key={f ?? "all"}
              type="button"
              role="radio"
              aria-checked={family === f}
              onClick={() => setFamily(f)}
              className={cn(
                "min-h-11 rounded-full border px-4 font-semibold",
                family === f ? "border-brand bg-brand-soft" : "border-line text-ink-2",
              )}
            >
              {t(`settings:audit.families.${f === null ? "all" : f.slice(0, -1)}`)}
            </button>
          ))}
        </div>
        {rows.length === 0 && !log.isPending ? (
          <p className="text-ink-2">{t("settings:audit.empty")}</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full border-collapse">
              <thead className="sticky top-0 bg-surface text-sm text-ink-2">
                <tr className="h-10">
                  <th className="text-end font-semibold">{t("settings:audit.columns.seq")}</th>
                  <th className="text-start font-semibold">{t("settings:audit.columns.at")}</th>
                  <th className="text-start font-semibold">{t("settings:audit.columns.user")}</th>
                  <th className="text-start font-semibold">{t("settings:audit.columns.action")}</th>
                  <th className="text-start font-semibold">{t("settings:audit.columns.details")}</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <tr key={row.seq} className="h-10 border-t border-line even:bg-surface-2">
                    <td className="text-end">
                      <bdi dir="ltr">{row.seq}</bdi>
                    </td>
                    <td>
                      <bdi dir="ltr">{formatDate(row.at, lang, { withTime: true })}</bdi>
                    </td>
                    <td>{row.userName ?? t("settings:audit.system")}</td>
                    <td>{actionLabel(row.action)}</td>
                    <td>
                      <code dir="ltr" className="block max-w-md truncate text-sm text-ink-2">
                        {JSON.stringify(row.payload)}
                      </code>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {log.hasNextPage ? (
          <div>
            <Button
              variant="outline"
              disabled={log.isFetchingNextPage}
              onClick={() => void log.fetchNextPage()}
            >
              {t("common:actions.loadMore")}
            </Button>
          </div>
        ) : null}
      </Card>
    </div>
  );
}
