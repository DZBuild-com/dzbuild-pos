import { Plus } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TruncatedNotice, capped } from "@/components/data-table/list-notices";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ImportLink } from "@/features/imports/import-entities";
import { useSession } from "@/lib/ipc/auth";
import { customerQuery, useCustomerSearch } from "@/lib/ipc/customers";
import type { CustomerHit, ExportTarget, Permission } from "@/lib/ipc/types";
import { useDebounced } from "@/lib/use-debounced";
import { cn } from "@/lib/utils";
import { CustomerForm } from "./CustomerForm";

type Filter = "owing" | "blocked" | "archived";
const FILTERS: readonly Filter[] = ["owing", "blocked", "archived"];

const col = columnHelper<CustomerHit>();

/**
 * CRM-001/010: the customer directory, searched by name, code, phone or NIF in any script, with
 * the kridi figures of each (the backend's balance, limit and credit still available; hidden
 * without `credit.view`). A row opens the customer's file. Only `customers.manage` exports the
 * directory (loi 18-07: personal data leaves the shop only with that right, and is journaled),
 * exactly as filtered here. "Archivés" adds the archived customers to the list.
 */
/** Customers listed at most; a search or a filter finds the others. */
const LIST_LIMIT = 500;

export function CustomersPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const permissions = useSession().data?.permissions ?? [];
  const can = (p: Permission) => permissions.includes(p);
  const seesCredit = can("credit.view");
  const [search, setSearch] = useState("");
  const [filters, setFilters] = useState<Filter[]>([]);
  const [creating, setCreating] = useState(false);
  const text = useDebounced(search.trim(), 200);
  const on = (f: Filter) => filters.includes(f);
  const query = customerQuery({
    text,
    withDebt: on("owing"),
    blocked: on("blocked") ? true : null,
    includeArchived: on("archived"),
  });
  // One more than shown: whether the list stops at its limit (the export takes the whole file).
  const hits = useCustomerSearch({ ...query, limit: LIST_LIMIT + 1 });
  const list = capped(hits.data, LIST_LIMIT);
  const exportTarget: ExportTarget | null = can("customers.manage") ? { kind: "customers", query } : null;

  const columns = useMemo(() => {
    const base = [
      col.accessor("code", {
        header: () => t("customers:fields.code"),
        cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
      }),
      col.accessor("name", {
        header: () => t("customers:fields.name"),
        cell: ({ row }) => (
          <span className="flex items-center gap-2">
            <span className="truncate font-semibold">{row.original.name}</span>
            {row.original.isBlocked ? <Badge tone="danger">{t("customers:status.blocked")}</Badge> : null}
            {row.original.archived ? <Badge>{t("customers:status.archived")}</Badge> : null}
          </span>
        ),
      }),
      col.accessor("phone", {
        header: () => t("customers:fields.phone"),
        enableSorting: false,
        cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
      }),
      col.accessor("customerType", {
        header: () => t("customers:fields.type"),
        cell: (c) => t(`customers:types.${c.getValue()}`),
      }),
    ];
    if (!seesCredit) return col.columns(base);
    return col.columns([
      ...base,
      col.accessor("balance", {
        header: () => t("customers:detail.balance"),
        meta: { align: "end" },
        cell: ({ row }) => {
          const { balance, creditLimit } = row.original;
          const over = creditLimit !== null && balance > creditLimit;
          return (
            <span className={cn(over && "font-semibold text-danger")}>
              <Amount value={balance} />
              {over ? <span className="sr-only"> {t("customers:status.overLimit")}</span> : null}
            </span>
          );
        },
      }),
      col.accessor("creditLimit", {
        header: () => t("customers:detail.limit"),
        meta: { align: "end" },
        cell: (c) => {
          const limit = c.getValue();
          return limit === null ? (
            <span className="text-ink-2">{t("customers:noLimit")}</span>
          ) : (
            <Amount value={limit} />
          );
        },
      }),
      col.accessor("availableCredit", {
        header: () => t("customers:detail.available"),
        meta: { align: "end" },
        cell: (c) => {
          const available = c.getValue();
          return available === null ? <span aria-hidden>—</span> : <Amount value={available} />;
        },
      }),
    ]);
  }, [t, seesCredit]);

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <h1 className="text-3xl font-bold">{t("customers:title")}</h1>
        <div className="flex flex-wrap gap-2">
          <ImportLink entities={["customers", "customer_balances"]} />
          {can("customers.manage") ? (
            <Button variant="primary" onClick={() => setCreating(true)}>
              <Plus aria-hidden />
              {t("customers:new")}
            </Button>
          ) : null}
        </div>
      </div>
      <TableToolbar
        search={search}
        onSearch={setSearch}
        onScan={setSearch}
        exportTarget={exportTarget}
        filters={
          <div role="group" aria-label={t("customers:filters.label")} className="flex flex-wrap gap-2">
            {FILTERS.filter((f) => f !== "owing" || seesCredit).map((f) => (
              <Button
                key={f}
                variant="outline"
                aria-pressed={on(f)}
                className={cn(on(f) && "border-brand bg-brand-soft hover:bg-brand-soft")}
                onClick={() => setFilters((all) => (on(f) ? all.filter((x) => x !== f) : [...all, f]))}
              >
                {t(`customers:filters.${f}`)}
              </Button>
            ))}
          </div>
        }
      />
      {list.truncated ? <TruncatedNotice rows={LIST_LIMIT} /> : null}
      <DataTable
        ariaLabel={t("customers:title")}
        data={list.rows}
        columns={columns}
        sortable={!list.truncated}
        getRowId={(row) => row.id}
        onRowClick={(row) => void navigate(`/customers/${row.id}`)}
        empty={hits.isPending ? t("common:status.loading") : t("customers:empty")}
      />
      {creating ? (
        <CustomerForm
          customer={null}
          onClose={() => setCreating(false)}
          onSaved={(customer) => void navigate(`/customers/${customer.id}`)}
        />
      ) : null}
    </div>
  );
}
