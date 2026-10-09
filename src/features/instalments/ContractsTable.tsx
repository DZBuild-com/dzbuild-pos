import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { Badge } from "@/components/ui/badge";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { usePlans } from "@/lib/ipc/instalments";
import type { Plan, PlanFilter } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { daysLate, nextDue } from "./kinds";

const col = columnHelper<Plan>();

/** The contracts of a filter (number, customer, total, paid, left, next due, late badge); a row
 * opens its contract at `/instalments/<id>`. */
export function ContractsTable({ filter }: { filter: PlanFilter }) {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const errorText = useErrorText();
  const plans = usePlans(filter);
  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("number", {
          header: () => t("instalments:list.number"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("customerName", { header: () => t("instalments:list.customer") }),
        col.accessor("total", {
          header: () => t("instalments:plan.total"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("paid", {
          header: () => t("instalments:plan.paid"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("remaining", {
          header: () => t("instalments:plan.left"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} className="font-semibold" />,
        }),
        col.accessor((p) => nextDue(p)?.dueOn ?? "", {
          id: "next",
          header: () => t("instalments:plan.next"),
          cell: (c) => (c.getValue() ? <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi> : "—"),
        }),
        col.accessor((p) => daysLate(p), {
          id: "late",
          header: () => t("instalments:plan.state"),
          cell: (c) =>
            c.getValue() > 0 ? (
              <Badge tone="danger">{t("instalments:plan.status.overdue", { count: c.getValue() })}</Badge>
            ) : null,
        }),
      ]),
    [t, lang],
  );
  if (plans.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(plans.error)}
      </p>
    );
  }
  return (
    <DataTable
      ariaLabel={t("instalments:list.label")}
      data={plans.data ?? []}
      columns={columns}
      getRowId={(p) => p.id}
      onRowClick={(p) => void navigate(`/instalments/${p.id}`)}
      empty={plans.isPending ? t("common:status.loading") : t("instalments:list.empty")}
    />
  );
}
