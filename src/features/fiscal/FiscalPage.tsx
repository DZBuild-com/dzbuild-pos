import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import type { Session } from "@/lib/ipc/types";
import { cn } from "@/lib/utils";
import { ArchiveExport } from "./ArchiveExport";
import { ArchiveVerify } from "./ArchiveVerify";
import { G50Helper } from "./G50Helper";
import { TaxReportsList } from "./TaxReportsList";

const TABS = ["g50", "registers", "archive"] as const;
type Tab = (typeof TABS)[number];

/** The month before `now`, `YYYY-MM`, in the webview's local calendar (a default, not a figure). */
export function previousMonth(now: Date = new Date()): string {
  const month = new Date(now.getFullYear(), now.getMonth() - 1, 1);
  return `${month.getFullYear()}-${String(month.getMonth() + 1).padStart(2, "0")}`;
}

/**
 * `/reports/fiscal` (with `reports.tax`): the G50 helper of a month, the tax registers and
 * statements, and the fiscal archive — its export needs `settings.data` too, its check does not.
 */
export function FiscalPage({ session }: { session: Session }) {
  const { t } = useTranslation("fiscal");
  const id = useId();
  const [params, setParams] = useSearchParams();
  const requested = params.get("tab");
  const tab: Tab = TABS.find((k) => k === requested) ?? "g50";
  const [month, setMonth] = useState(() => previousMonth());
  const exportsArchive = session.permissions.includes("settings.data");

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-3xl font-bold">{t("title")}</h1>
      <div role="tablist" aria-label={t("title")} className="flex flex-wrap gap-2 border-b border-line pb-2">
        {TABS.map((k) => (
          <button
            key={k}
            type="button"
            role="tab"
            id={`${id}-tab-${k}`}
            aria-selected={tab === k}
            aria-controls={`${id}-panel`}
            onClick={() => setParams({ tab: k }, { replace: true })}
            className={cn(
              "min-h-11 rounded-control px-4 font-semibold",
              tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
            )}
          >
            {t(`tabs.${k}`)}
          </button>
        ))}
      </div>
      <div
        role="tabpanel"
        id={`${id}-panel`}
        aria-labelledby={`${id}-tab-${tab}`}
        className="flex flex-col gap-4"
      >
        {tab === "g50" ? (
          <>
            <div className="max-w-xs">
              <Field id={`${id}-month`} label={t("g50.month")}>
                <Input
                  id={`${id}-month`}
                  type="month"
                  dir="ltr"
                  value={month}
                  onChange={(event) => setMonth(event.target.value)}
                />
              </Field>
            </div>
            <G50Helper month={month} />
          </>
        ) : null}
        {tab === "registers" ? <TaxReportsList /> : null}
        {tab === "archive" ? (
          <>
            {exportsArchive ? <ArchiveExport /> : null}
            <ArchiveVerify />
          </>
        ) : null}
      </div>
    </div>
  );
}
