import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { Badge } from "@/components/ui/badge";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useOrders } from "@/lib/ipc/orders";
import type { CustomerOrderFilter, OrderSummary } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const col = columnHelper<OrderSummary>();

/** The orders of a filter (number, kind, customer, due date, total, held, status, days open); a row
 * opens the order at `/customer-orders/<id>`. */
export function OrdersTable({ filter }: { filter: CustomerOrderFilter }) {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const errorText = useErrorText();
  const orders = useOrders(filter);
  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("number", {
          header: () => t("orders:list.number"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("kind", {
          header: () => t("orders:list.kind"),
          cell: (c) => t(`orders:kinds.${c.getValue()}`),
        }),
        col.accessor("customerName", { header: () => t("orders:list.customer") }),
        col.accessor((o) => o.dueOn ?? "", {
          id: "dueOn",
          header: () => t("orders:dialog.due"),
          cell: (c) => (c.getValue() ? <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi> : "—"),
        }),
        col.accessor("total", {
          header: () => t("orders:list.total"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("held", {
          header: () => t("orders:list.held"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("status", {
          header: () => t("orders:list.status"),
          cell: (c) => (
            <Badge tone={c.getValue() === "ready" ? "ok" : c.getValue() === "open" ? "info" : "neutral"}>
              {t(`orders:status.${c.getValue()}`)}
            </Badge>
          ),
        }),
        col.accessor("daysOpen", {
          header: () => t("orders:list.daysOpen"),
          meta: { align: "end" },
          cell: (c) => t("orders:list.days", { count: c.getValue() }),
        }),
      ]),
    [t, lang],
  );
  if (orders.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(orders.error)}
      </p>
    );
  }
  return (
    <DataTable
      ariaLabel={t("orders:list.label")}
      data={orders.data ?? []}
      columns={columns}
      getRowId={(o) => o.id}
      onRowClick={(o) => void navigate(`/customer-orders/${o.id}`)}
      empty={orders.isPending ? t("common:status.loading") : t("orders:list.empty")}
    />
  );
}
