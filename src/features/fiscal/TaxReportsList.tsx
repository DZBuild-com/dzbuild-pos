import { ChevronRight, FileSpreadsheet } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { Splash } from "@/components/splash";
import { useReportCatalog } from "@/lib/ipc/reports";
import type { ReportKind } from "@/lib/ipc/types";
import { useLang } from "@/lib/use-language";

/** The tax registers and statements (RPT-046…053), opened in M2's report screen with its filters
 * and its xlsx/csv/PDF exports. */
export const TAX_REPORTS = [
  "vat_by_rate",
  "vat_deductible",
  "stamp_duty_statement",
  "g50",
  "ifu_turnover",
  "g9_suppliers",
  "customers_annual",
  "sales_register",
  "purchases_register",
] as const satisfies readonly ReportKind[];

/** One link per tax report: its title from the backend's catalogue, a line saying what it holds. */
export function TaxReportsList() {
  const { t } = useTranslation("fiscal");
  const lang = useLang();
  const catalog = useReportCatalog(lang);
  if (catalog.isPending) return <Splash inline />;
  const titles = new Map((catalog.data ?? []).map((report) => [report.kind, report.title]));
  return (
    <section className="flex flex-col gap-3">
      <p className="text-ink-2">{t("registers.intro")}</p>
      <ul className="grid gap-2 lg:grid-cols-2">
        {TAX_REPORTS.map((kind) => {
          const title = titles.get(kind);
          return (
            <li key={kind}>
              <Link
                to={`/reports/${kind}`}
                className="flex min-h-14 items-center gap-3 rounded-card border border-line bg-surface px-4 py-3 hover:bg-brand-soft focus-visible:outline-3 focus-visible:outline-focus"
              >
                <FileSpreadsheet aria-hidden className="size-5 shrink-0 text-ink-2" />
                <span className="flex flex-col">
                  <span className="font-semibold">{title ?? t(`registers.kinds.${kind}`)}</span>
                  {title ? <span className="text-sm text-ink-2">{t(`registers.kinds.${kind}`)}</span> : null}
                </span>
                <ChevronRight aria-hidden className="ms-auto size-5 shrink-0 text-ink-2 rtl:-scale-x-100" />
              </Link>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
