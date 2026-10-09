import { Download, ScanSearch } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { StatTile } from "@/components/stat-tile";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { formatNumber } from "@/lib/format";
import { useSaveErrorReport } from "@/lib/ipc/imports";
import type { DryRun, FieldInfo, ImportEntity, ImportPolicy, Problem } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

type Row = Problem & { id: string };
const col = columnHelper<Row>();

/**
 * IMX-006, step 3: the dry run (nothing is kept) with what it would create, update, skip, refuse
 * and, for a sync, archive; every problem with its row, column, value and reason in the UI
 * language, and the same list as an xlsx error report. `dry` is the check of the current mapping
 * only: a mapping changed since the check shows `stale` instead.
 */
export function CheckStep({
  entity,
  fields,
  policy,
  dry,
  stale,
  canCheck,
  pending,
  onCheck,
  error,
}: {
  entity: ImportEntity;
  fields: FieldInfo[];
  policy: ImportPolicy;
  dry: DryRun | null;
  stale: boolean;
  canCheck: boolean;
  pending: boolean;
  onCheck: () => void;
  error: unknown;
}) {
  const { t } = useTranslation();
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("imports:steps.check")}</h2>
      <p className="text-ink-2">{t("imports:check.intro")}</p>
      <div>
        <Button variant="secondary" disabled={!canCheck || pending} onClick={onCheck}>
          <ScanSearch aria-hidden />
          {t("imports:check.run")}
        </Button>
      </div>
      {error ? <ErrorLine error={error} /> : null}
      {stale ? (
        <p role="status" className="rounded-control bg-warn-soft p-3 text-warn">
          {t("imports:check.changed")}
        </p>
      ) : null}
      {dry ? <CheckResult entity={entity} fields={fields} policy={policy} dry={dry} /> : null}
    </Card>
  );
}

function ErrorLine({ error }: { error: unknown }) {
  const errorText = useErrorText();
  return (
    <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
      {errorText(error)}
    </p>
  );
}

function CheckResult({
  entity,
  fields,
  policy,
  dry,
}: {
  entity: ImportEntity;
  fields: FieldInfo[];
  policy: ImportPolicy;
  dry: DryRun;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const report = useSaveErrorReport();
  const counts = [
    ["total", dry.total],
    ["create", dry.create],
    ["update", dry.update],
    ["skip", dry.skip],
    ["fail", dry.fail],
    ...(policy === "sync" ? [["archive", dry.archive] as const] : []),
  ] as const;
  const rows = useMemo(() => dry.problems.map((p, i) => ({ ...p, id: String(i) })), [dry.problems]);
  const columns = useMemo(() => {
    const labelOf = (key: string | null) => (key ? (fields.find((f) => f.key === key)?.label ?? key) : "");
    return col.columns([
      col.accessor("row", {
        header: () => t("imports:check.columns.row"),
        cell: (c) => t("imports:check.row", { row: c.getValue() }),
      }),
      col.accessor((p) => labelOf(p.field), { id: "field", header: () => t("imports:check.columns.field") }),
      col.accessor("value", {
        header: () => t("imports:check.columns.value"),
        enableSorting: false,
        cell: (c) => <bdi dir="auto">{c.getValue()}</bdi>,
      }),
      col.accessor((p) => t(`imports:problems.${p.code}`), {
        id: "problem",
        header: () => t("imports:check.columns.problem"),
        cell: (c) => <span title={c.getValue()}>{c.getValue()}</span>,
      }),
    ]);
  }, [t, fields]);
  const saveReport = () =>
    report.mutate(
      { entity, problems: dry.problems, lang },
      {
        // `null`: the save dialog was cancelled.
        onSuccess: (path) => {
          if (path) toast.success(t("imports:check.reportSaved"));
        },
        onError: (error) => toast.error(errorText(error)),
      },
    );
  return (
    <>
      <div
        role="group"
        aria-label={t("imports:check.counts.label")}
        className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6"
      >
        {counts.map(([key, value]) => (
          <StatTile key={key} label={t(`imports:check.counts.${key}`)} value={formatNumber(value, lang)} />
        ))}
      </div>
      {policy === "sync" && dry.stockKept > 0 ? (
        <div role="status" className="flex flex-col gap-1 rounded-control bg-warn-soft p-3 text-warn">
          <p className="font-semibold">
            {t("imports:check.stockKept", { n: formatNumber(dry.stockKept, lang) })}
          </p>
          <ul className="list-inside list-disc">
            {dry.stockKeptNames.map((name) => (
              <li key={name}>
                <bdi dir="auto">{name}</bdi>
              </li>
            ))}
          </ul>
          {dry.stockKept > dry.stockKeptNames.length ? <p>{t("imports:check.stockKeptMore")}</p> : null}
        </div>
      ) : null}
      {dry.problems.length === 0 ? (
        <p className="text-ok">{t("imports:check.none")}</p>
      ) : (
        <>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h3 className="text-lg font-bold">{t("imports:check.problems")}</h3>
            <Button variant="outline" disabled={report.isPending} onClick={saveReport}>
              <Download aria-hidden />
              {t("imports:check.report")}
            </Button>
          </div>
          {dry.problemsTruncated ? <p className="text-warn">{t("imports:check.truncated")}</p> : null}
          <DataTable
            ariaLabel={t("imports:check.problems")}
            data={rows}
            columns={columns}
            getRowId={(row) => row.id}
          />
        </>
      )}
    </>
  );
}
