import { ArrowLeft, FileX } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useParams, useSearchParams } from "react-router";
import { type BarRow, BarChart } from "@/components/bar-chart";
import { formatCell } from "@/components/data-table/cells";
import { ExportMenu } from "@/components/data-table/export-menu";
import { ReportTable } from "@/components/data-table/report-table";
import { EmptyState } from "@/components/empty-state";
import { Splash } from "@/components/splash";
import { formatNumber } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import { useReport, useReportCatalog } from "@/lib/ipc/reports";
import type { ReportInfo, ReportRequest, ReportView as View } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { type FilterState, ReportFilters, initialFilters, missingFilter, requestOf } from "./ReportFilters";

/** Ranked names drawn at most (the table keeps every row). */
const CHART_ROWS = 20;

/**
 * `/reports/:kind` (RPT-001…054): the kind's filters, then its chart when the backend draws one,
 * the table with its totals, a notice when the rows were cut at the limit, and the exports of
 * exactly what is shown (PDF waits for M3's print host). A kind the role may not run shows why.
 */
export function ReportView() {
  const { t } = useTranslation();
  const lang = useLang();
  const { kind } = useParams();
  const catalog = useReportCatalog(lang);
  const info = catalog.data?.find((r) => r.kind === kind);
  if (catalog.isPending) return <Splash inline />;
  if (!info?.allowed) {
    return (
      <div className="flex flex-col gap-4">
        <BackLink />
        <EmptyState
          icon={FileX}
          title={t("reports:unavailable.title")}
          body={t("reports:unavailable.body")}
        />
      </div>
    );
  }
  return <ReportScreen key={info.kind} info={info} />;
}

function BackLink() {
  const { t } = useTranslation();
  return (
    <Link
      to="/reports"
      className="inline-flex min-h-11 items-center gap-2 self-start font-semibold text-ink-2 hover:text-ink"
    >
      <ArrowLeft aria-hidden className="size-5 rtl:-scale-x-100" />
      {t("reports:back")}
    </Link>
  );
}

function ReportScreen({ info }: { info: ReportInfo }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [params] = useSearchParams();
  const [filters, setFilters] = useState<FilterState>(() => initialFilters(params));
  const request = requestOf(info, filters);
  // the names in the rows come in the UI language; an export sets its own
  const report = useReport(request ? { ...request, lang } : null);
  const missing = missingFilter(info, filters);
  const view = request ? report.data : undefined;

  let body = null;
  if (missing) {
    body = (
      <p className="rounded-card border border-dashed border-line p-6 text-ink-2">
        {t(`reports:required.${missing}`)}
      </p>
    );
  } else if (request && report.error) {
    body = (
      <p role="alert" className="text-danger">
        {errorText(report.error)}
      </p>
    );
  } else if (request && view) {
    body = <ReportBody view={view} request={request} />;
  } else if (request) {
    body = <Splash inline />;
  }

  return (
    <div className="flex flex-col gap-4">
      <BackLink />
      <div className="flex flex-col gap-1">
        <h1 className="text-3xl font-bold">{info.title}</h1>
        {view ? <p className="text-ink-2">{view.subtitle}</p> : null}
      </div>
      <ReportFilters info={info} value={filters} onChange={setFilters} />
      {body}
    </div>
  );
}

function ReportBody({ view, request }: { view: View; request: ReportRequest }) {
  const { t } = useTranslation();
  const lang = useLang();
  const chart = chartOf(view, lang);
  return (
    <div className="flex flex-col gap-4">
      {chart ? (
        <div className="flex flex-col gap-2 rounded-card border border-line bg-surface p-4">
          <BarChart
            title={t("reports:chart.title", { value: chart.valueLabel })}
            kind={chart.kind}
            rows={chart.rows}
            formatValue={chart.formatValue}
          />
          {chart.cut ? (
            <p className="text-sm text-ink-2">
              {t("reports:chart.first", { rows: formatNumber(chart.rows.length, lang) })}
            </p>
          ) : null}
        </div>
      ) : null}
      {view.report.truncated ? (
        <p role="status" className="rounded-control border border-warn bg-warn-soft p-3 text-warn">
          {t("reports:truncated", { rows: formatNumber(view.report.table.rows.length, lang) })}
        </p>
      ) : null}
      <div className="flex justify-end">
        <ExportMenu target={{ kind: "report", request }} />
      </div>
      <ReportTable view={view} />
    </div>
  );
}

/** The backend's chart hint read from the table: its label and value columns (formatted like the
 * table cells), ranked names cut to {@link CHART_ROWS}. */
function chartOf(view: View, lang: Lang) {
  const chart = view.report.chart;
  if (!chart) return null;
  const { columns, rows } = view.report.table;
  const labelAt = columns.findIndex((c) => c.key === chart.label);
  const valueAt = columns.findIndex((c) => c.key === chart.value);
  const label = columns[labelAt];
  const value = columns[valueAt];
  if (!label || !value) return null;
  const all: BarRow[] = rows.flatMap((row) => {
    const cell = row[valueAt];
    if (typeof cell !== "number") return [];
    const name = formatCell(label.kind, row[labelAt] ?? null, lang, label.tag ?? null, view.tagLabels);
    return [{ label: name, value: cell }];
  });
  const ranked = chart.kind === "h_bar";
  return {
    kind: ranked ? ("hbar" as const) : ("bar" as const),
    rows: ranked ? all.slice(0, CHART_ROWS) : all,
    cut: ranked && all.length > CHART_ROWS,
    valueLabel: view.labels[valueAt] ?? value.key,
    formatValue: (v: number) => formatCell(value.kind, v, lang),
  };
}
