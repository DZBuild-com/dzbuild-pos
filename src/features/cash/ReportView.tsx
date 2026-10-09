import { Printer } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { SessionSummary, ZReport } from "@/lib/ipc/types";

function Row({ label, value, strong }: { label: string; value: number; strong?: boolean }) {
  return (
    <div className={strong ? "flex justify-between font-bold" : "flex justify-between"}>
      <dt>{label}</dt>
      <dd>
        <Amount value={value} />
      </dd>
    </div>
  );
}

/** An X or Z report on screen (CSH-007, CSH-008): totals, tenders, drawer, count and integrity. */
export function ReportView({
  summary,
  z,
  onPrint,
  printing,
}: {
  summary: SessionSummary;
  z?: ZReport;
  onPrint: () => void;
  printing: boolean;
}) {
  const { t } = useTranslation();
  const intact = z ? z.auditAnchor.intact && z.documentAnchors.every((d) => d.intact) : true;
  return (
    <div className="flex flex-col gap-4">
      {z ? (
        <p className="text-2xl font-bold">
          {t("cash:report.z")} <bdi dir="ltr">{z.zNumber}</bdi>
        </p>
      ) : (
        <p className="text-2xl font-bold">{t("cash:report.x")}</p>
      )}
      {z?.offlinePending !== undefined && z.offlinePending !== null ? (
        <p role="note" className="flex flex-wrap items-center gap-2 rounded-control bg-warn-soft p-3">
          <Badge tone="warn">{t("sync:lan.zList.offline")}</Badge>
          {t("sync:lan.toSend", { count: z.offlinePending })}
        </p>
      ) : null}
      {summary.blind ? (
        <p role="note" className="rounded-control bg-info-soft p-3 text-info">
          {t("cash:report.blind")}
        </p>
      ) : null}
      <dl className="grid gap-1">
        <div className="flex justify-between">
          <dt>{t("cash:report.tickets")}</dt>
          <dd>
            <bdi dir="ltr">{summary.tickets}</bdi>
          </dd>
        </div>
        {summary.blind ? null : (
          <>
            <Row label={t("cash:report.sales")} value={summary.salesTtc} />
            <Row label={t("cash:report.returns")} value={summary.returnsTtc} />
            {/* M3: the invoices and credit notes of the session, whose payments are below */}
            {summary.invoices ? (
              <Row
                label={t("cash:report.invoices", { number: summary.invoices })}
                value={summary.invoicesTtc ?? 0}
              />
            ) : null}
            {summary.creditNotes ? (
              <Row
                label={t("cash:report.creditNotes", { number: summary.creditNotes })}
                value={summary.creditNotesTtc ?? 0}
              />
            ) : null}
            <Row label={t("cash:report.net")} value={summary.netTtc} strong />
            {summary.stampCharged !== 0 ? (
              <Row label={t("cash:report.stampDuty")} value={summary.stampCharged} />
            ) : null}
          </>
        )}
      </dl>
      {summary.byMethod.length > 0 ? (
        <dl className="grid gap-1 border-t border-line pt-3">
          {summary.byMethod.map((m) => (
            <Row key={m.method} label={t(`payments:tenders.${m.method}`)} value={m.net} />
          ))}
        </dl>
      ) : null}
      {summary.blind ? null : (
        <dl className="grid gap-1 border-t border-line pt-3">
          <Row label={t("cash:report.float")} value={summary.openingFloat} />
          <Row label={t("cash:report.paidIn")} value={summary.paidIn} />
          <Row label={t("cash:report.paidOut")} value={summary.paidOut} />
          {/* A Z stored before M2 has no field: it read 0 there. */}
          <Row label={t("cash:report.creditPayments")} value={summary.creditPayments ?? 0} />
          <Row label={t("cash:report.expenses")} value={summary.expensesTotal} />
          {summary.expectedCash !== null ? (
            <Row label={t("cash:report.expected")} value={summary.expectedCash} strong />
          ) : null}
          {z ? <Row label={t("cash:report.counted")} value={z.countedCash} strong /> : null}
          {z ? <Row label={t("cash:report.variance")} value={z.variance} strong /> : null}
        </dl>
      )}
      {z ? (
        <p
          role="status"
          className={
            intact
              ? "rounded-control bg-ok-soft p-3 font-semibold text-ok"
              : "rounded-control bg-danger-soft p-3 font-semibold text-danger"
          }
        >
          {intact ? t("cash:report.intact") : t("cash:report.broken")}
        </p>
      ) : null}
      <div>
        <Button variant="outline" onClick={onPrint} disabled={printing}>
          <Printer aria-hidden />
          {z ? t("cash:report.printZ") : t("cash:report.printX")}
        </Button>
      </div>
    </div>
  );
}
