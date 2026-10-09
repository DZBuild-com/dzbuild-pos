import { ChartColumn, ChevronRight, Landmark } from "lucide-react";
import { useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { Splash } from "@/components/splash";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useSession } from "@/lib/ipc/auth";
import { useReportCatalog } from "@/lib/ipc/reports";
import type { ReportGroup, ReportInfo } from "@/lib/ipc/types";
import { useLang } from "@/lib/use-language";

/** Lower case without accents or Arabic vowel marks: "declaration" finds "Déclaration". */
function fold(text: string): string {
  return text.normalize("NFD").replace(/\p{M}/gu, "").toLocaleLowerCase();
}

/**
 * `/reports`: the catalogue of the reports the role may run, grouped like the backend groups them
 * (sales, margins, stock, purchases, customers, cash, declarations), found by name or group; with
 * `reports.tax`, the fiscal screen (G50, registers, fiscal archive) first.
 */
export function ReportsPage() {
  const { t } = useTranslation();
  const lang = useLang();
  const id = useId();
  const catalog = useReportCatalog(lang);
  const fiscal = useSession().data?.permissions.includes("reports.tax") ?? false;
  const [search, setSearch] = useState("");
  const needle = fold(search.trim());
  const allowed = useMemo(() => (catalog.data ?? []).filter((r) => r.allowed), [catalog.data]);
  const groups = useMemo(() => {
    const byGroup = new Map<ReportGroup, ReportInfo[]>();
    for (const report of allowed) {
      if (needle !== "" && ![report.title, report.groupTitle].some((text) => fold(text).includes(needle))) {
        continue;
      }
      byGroup.set(report.group, [...(byGroup.get(report.group) ?? []), report]);
    }
    return [...byGroup.entries()];
  }, [allowed, needle]);

  if (catalog.isPending) return <Splash inline />;
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-3xl font-bold">{t("reports:title")}</h1>
      <div className="max-w-md">
        <Field id={`${id}-search`} label={t("reports:search")}>
          <Input
            id={`${id}-search`}
            type="search"
            dir="auto"
            autoComplete="off"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </Field>
      </div>
      {fiscal ? (
        <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
          <Link
            to="/reports/fiscal"
            className="flex min-h-14 items-center gap-3 rounded-card border border-line bg-surface px-4 py-3 font-semibold hover:bg-brand-soft focus-visible:outline-3 focus-visible:outline-focus"
          >
            <Landmark aria-hidden className="size-5 shrink-0 text-ink-2" />
            {t("fiscal:title")}
            <ChevronRight aria-hidden className="ms-auto size-5 shrink-0 text-ink-2 rtl:-scale-x-100" />
          </Link>
        </div>
      ) : null}
      {allowed.length === 0 ? <p className="text-ink-2">{t("reports:none")}</p> : null}
      {allowed.length > 0 && groups.length === 0 ? (
        <p className="text-ink-2">{t("reports:noMatch")}</p>
      ) : null}
      {groups.map(([group, reports]) => (
        <section key={group} aria-labelledby={`${id}-${group}`} className="flex flex-col gap-3">
          <h2 id={`${id}-${group}`} className="text-xl font-bold">
            {reports[0]?.groupTitle}
          </h2>
          <ul className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
            {reports.map((report) => (
              <li key={report.kind}>
                <Link
                  to={`/reports/${report.kind}`}
                  className="flex min-h-14 items-center gap-3 rounded-card border border-line bg-surface px-4 py-3 font-semibold hover:bg-brand-soft focus-visible:outline-3 focus-visible:outline-focus"
                >
                  <ChartColumn aria-hidden className="size-5 shrink-0 text-ink-2" />
                  {report.title}
                  <ChevronRight aria-hidden className="ms-auto size-5 shrink-0 text-ink-2 rtl:-scale-x-100" />
                </Link>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
