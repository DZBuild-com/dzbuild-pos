import { LayoutDashboard } from "lucide-react";
import { type ReactNode, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { Amount } from "@/components/amount";
import { BarChart } from "@/components/bar-chart";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { EmptyState } from "@/components/empty-state";
import { Splash } from "@/components/splash";
import { StatTile } from "@/components/stat-tile";
import { Card } from "@/components/ui/card";
import { type DateRange, dayToMs, rangeOf } from "@/lib/dates";
import { formatDate, formatDZD, formatNumber } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { useDashboard, useReportCatalog } from "@/lib/ipc/reports";
import type { MethodNet, ReportKind, TopLine } from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** One figure: its `reports:dashboard.*` key, the value (`null`: hidden by the backend), whether it
 * is money or a count, and the report that details it over `range` (today by default). */
interface Tile {
  key: string;
  value: number | null;
  money: boolean;
  report: ReportKind;
  range?: DateRange;
}

const topCol = columnHelper<TopLine>();
const methodCol = columnHelper<MethodNet>();

/**
 * `/dashboard` (RPT-060, UX-030), with `reports.sales`: today's figures (a figure the role may not
 * see comes back `null` and has no tile), the sales of the last 14 days, today's best sellers and
 * takings per payment method. Every part opens the report that details it when the role may run it.
 */
export function DashboardPage() {
  const { t } = useTranslation();
  const session = useSession().data;
  if (session === undefined) return <Splash inline />;
  if (!session?.permissions.includes("reports.sales")) {
    return (
      <EmptyState
        icon={LayoutDashboard}
        title={t("reports:dashboard.noAccess")}
        body={t("reports:dashboard.noAccessBody")}
      />
    );
  }
  return <Dashboard />;
}

function Dashboard() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const dashboard = useDashboard();
  const catalog = useReportCatalog(lang).data;
  const topColumns = useMemo(
    () =>
      topCol.columns([
        topCol.accessor("name", { header: () => t("reports:dashboard.columns.product") }),
        topCol.accessor("qty", {
          header: () => t("reports:dashboard.columns.qty"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{formatQty(c.getValue(), lang)}</bdi>,
        }),
        topCol.accessor("netTtc", {
          header: () => t("reports:dashboard.columns.netTtc"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
      ]),
    [t, lang],
  );
  const methodColumns = useMemo(
    () =>
      methodCol.columns([
        methodCol.accessor((m) => t(`payments:tenders.${m.method}`), {
          id: "method",
          header: () => t("reports:dashboard.columns.method"),
        }),
        methodCol.accessor("net", {
          header: () => t("reports:dashboard.columns.net"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
      ]),
    [t],
  );

  const d = dashboard.data;
  if (dashboard.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(dashboard.error)}
      </p>
    );
  }
  if (!d) return <Splash inline />;

  const day = d.businessDate;
  /** The report's address over `range`, or `null` when the role may not run it. */
  const linkTo = (report: ReportKind, range: DateRange = { from: day, to: day }) =>
    catalog?.some((r) => r.kind === report && r.allowed)
      ? `/reports/${report}?${new URLSearchParams({ from: range.from, to: range.to }).toString()}`
      : null;
  const tiles: Tile[] = [
    { key: "today", value: d.netTtc, money: true, report: "sales_summary" },
    { key: "tickets", value: d.tickets, money: false, report: "sales_summary" },
    { key: "averageTicket", value: d.averageTicket, money: true, report: "sales_summary" },
    { key: "returns", value: d.returnsTtc, money: true, report: "returns" },
    {
      key: "yesterday",
      value: d.yesterdayNetTtc,
      money: true,
      report: "sales_summary",
      range: rangeOf("yesterday", day),
    },
    {
      key: "month",
      value: d.monthNetTtc,
      money: true,
      report: "sales_summary",
      range: rangeOf("this_month", day),
    },
    { key: "margin", value: d.margin, money: true, report: "profit_by_period" },
    { key: "receivables", value: d.receivables, money: true, report: "customer_balances" },
    { key: "payables", value: d.payables, money: true, report: "supplier_balances" },
    { key: "lowStock", value: d.lowStock, money: false, report: "low_stock" },
    { key: "expiring", value: d.expiring, money: false, report: "expiry" },
    { key: "openTills", value: d.openSessions, money: false, report: "cash_sessions" },
  ];
  const firstDay = d.lastDays[0]?.day ?? day;

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        <h1 className="text-3xl font-bold">{t("nav:dashboard")}</h1>
        <p className="text-ink-2">{t("reports:dashboard.day", { date: formatDate(dayToMs(day), lang) })}</p>
      </div>
      <div
        data-testid="dashboard-tiles"
        className="grid grid-cols-[repeat(auto-fill,minmax(14rem,1fr))] gap-3"
      >
        {tiles.flatMap(({ key, value, money, report, range }) => {
          if (value === null) return [];
          const tile = (
            <StatTile
              label={t(`reports:dashboard.${key}`)}
              value={
                money ? (
                  <Amount value={value} />
                ) : (
                  <bdi dir="ltr" className="tabular-nums">
                    {formatNumber(value, lang)}
                  </bdi>
                )
              }
            />
          );
          const to = linkTo(report, range);
          return [
            to ? (
              <Link
                key={key}
                to={to}
                className="rounded-card focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-focus [&:hover>dl]:bg-brand-soft"
              >
                {tile}
              </Link>
            ) : (
              <div key={key}>{tile}</div>
            ),
          ];
        })}
      </div>
      <Card>
        <BarChart
          title={t("reports:dashboard.lastDays")}
          kind="bar"
          rows={d.lastDays.map((total) => ({
            label: formatDate(dayToMs(total.day), lang),
            value: total.netTtc,
          }))}
          // whole dinars: the axis stays short and the exact amounts are in the report
          formatValue={(value) => formatDZD(value, lang, { hideCentimes: true })}
        />
        <ReportLink to={linkTo("sales_summary", { from: firstDay, to: day })} />
      </Card>
      <div className="grid gap-4 lg:grid-cols-2">
        <Section title={t("reports:dashboard.top")} to={linkTo("top_products")}>
          <DataTable
            ariaLabel={t("reports:dashboard.top")}
            data={d.topProducts}
            columns={topColumns}
            getRowId={(line) => line.productId ?? line.name}
            empty={t("reports:dashboard.noSales")}
          />
        </Section>
        <Section title={t("reports:dashboard.methods")} to={linkTo("sales_by_method")}>
          <DataTable
            ariaLabel={t("reports:dashboard.methods")}
            data={d.byMethod}
            columns={methodColumns}
            getRowId={(m) => m.method}
            empty={t("reports:dashboard.noSales")}
          />
        </Section>
      </div>
    </div>
  );
}

function Section({ title, to, children }: { title: string; to: string | null; children: ReactNode }) {
  return (
    <Card>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-xl font-bold">{title}</h2>
        <ReportLink to={to} />
      </div>
      {children}
    </Card>
  );
}

function ReportLink({ to }: { to: string | null }) {
  const { t } = useTranslation();
  if (!to) return null;
  return (
    <Link
      to={to}
      className="inline-flex min-h-11 items-center self-end font-semibold text-info underline-offset-4 hover:underline"
    >
      {t("reports:dashboard.openReport")}
    </Link>
  );
}
