import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { Cell, ReportView } from "@/lib/ipc/types";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { ReportCell, formatCell, isNumericKind } from "./cells";
import { type Columns, DataTable, columnHelper, gridColumns } from "./data-table";

interface ReportRow {
  id: string;
  cells: Cell[];
}

const helper = columnHelper<ReportRow>();

/** Any backend report: the column labels come with it, cells are formatted by column kind and
 * the totals row stays pinned under the rows. */
export function ReportTable({ view }: { view: ReportView }) {
  const { t } = useTranslation();
  const lang = useLang();
  const { columns: specs, rows, totals } = view.report.table;
  const columns = useMemo<Columns<ReportRow>>(
    () =>
      specs.map((spec, i) =>
        helper.accessor((row) => row.cells[i] ?? null, {
          id: spec.key,
          header: view.labels[i] ?? spec.key,
          cell: (info) => (
            <ReportCell kind={spec.kind} tag={spec.tag} value={info.getValue()} tagLabels={view.tagLabels} />
          ),
          meta: { align: isNumericKind(spec.kind) ? "end" : "start" },
        }),
      ),
    [specs, view.labels, view.tagLabels],
  );
  const data = useMemo(() => rows.map((cells, index) => ({ id: String(index), cells })), [rows]);

  let footer = null;
  if (totals) {
    const shown = specs.flatMap((spec, i) => {
      const text = formatCell(spec.kind, totals[i] ?? null, lang, spec.tag ?? null, view.tagLabels);
      return text === "" ? [] : [`${view.labels[i] ?? spec.key} ${text}`];
    });
    footer = (
      <div
        role="row"
        aria-label={[t("tables:total"), ...shown].join(", ")}
        className="grid min-h-11 items-center"
        style={{ gridTemplateColumns: gridColumns(specs.length) }}
      >
        {specs.map((spec, i) => (
          <div
            role="cell"
            key={spec.key}
            className={cn("truncate px-3 text-sm", isNumericKind(spec.kind) && "text-end tabular-nums")}
          >
            {i === 0 && (totals[0] ?? null) === null ? (
              t("tables:total")
            ) : (
              <ReportCell
                kind={spec.kind}
                tag={spec.tag}
                value={totals[i] ?? null}
                tagLabels={view.tagLabels}
              />
            )}
          </div>
        ))}
      </div>
    );
  }

  return (
    <DataTable
      ariaLabel={view.title}
      data={data}
      columns={columns}
      getRowId={(row) => row.id}
      footer={footer}
    />
  );
}
