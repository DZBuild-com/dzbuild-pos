import { Plus, Undo2 } from "lucide-react";
import { type FormEvent, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TruncatedNotice, capped } from "@/components/data-table/list-notices";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { DateRangePicker } from "@/components/date-range";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { useCan } from "@/features/stock/stock-parts";
import { type DateRange, dayToMs, rangeOf, today } from "@/lib/dates";
import { formatDate, formatDZD } from "@/lib/format";
import { useExpenseCategories, useExpenses, useExpenseTotals, useReverseExpense } from "@/lib/ipc/expenses";
import type { Expense, ExportTarget } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { ExpenseDialog } from "./ExpenseDialog";

const col = columnHelper<Expense>();
/** Expenses listed at most for a period; a shorter period or a category finds the others. */
const LIST_LIMIT = 1_000;

/**
 * FIN-001/002: the expenses of a period (this month by default), by category, with the net total
 * of each category from the backend and the export of the list (`reports.cash`). "Nouvelle
 * dépense" records one (`cash.paid_in_out`); a mistake is reversed with a note, never deleted.
 */
export function ExpensesTab() {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const [range, setRange] = useState<DateRange>(() => rangeOf("this_month", today()));
  const [categoryId, setCategoryId] = useState("");
  const [search, setSearch] = useState("");
  const [creating, setCreating] = useState(false);
  const [reversing, setReversing] = useState<Expense | null>(null);
  const categories = useExpenseCategories(true).data ?? [];
  const query = {
    from: range.from || null,
    to: range.to || null,
    categoryId: categoryId || null,
    limit: null,
  };
  // One more than shown: whether the period holds more expenses than the list shows.
  const expenses = useExpenses({ ...query, limit: LIST_LIMIT + 1 });
  const loaded = capped(expenses.data, LIST_LIMIT);
  const totals = useExpenseTotals(range.from, range.to, range.from !== "" && range.to !== "");
  const records = can("cash.paid_in_out");
  const exportTarget: ExportTarget | null = can("reports.cash") ? { kind: "expenses", query } : null;

  const text = search.trim().toLowerCase();
  const rows = useMemo(
    () =>
      loaded.rows.filter(
        (e) =>
          text === "" ||
          [localizedName(e, lang), e.reference ?? "", e.note ?? ""].some((v) =>
            v.toLowerCase().includes(text),
          ),
      ),
    [loaded.rows, text, lang],
  );
  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("businessDate", {
          header: () => t("cash:expenses.columns.date"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        col.accessor((e) => localizedName(e, lang), {
          id: "category",
          header: () => t("cash:expenses.columns.category"),
        }),
        col.accessor("amount", {
          header: () => t("cash:expenses.columns.amount"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor((e) => t(`payments:tenders.${e.method}`), {
          id: "method",
          header: () => t("cash:expenses.columns.method"),
          cell: ({ row, getValue }) => (
            <span className="flex items-center gap-2">
              {getValue()}
              {row.original.fromTill ? <Badge tone="info">{t("cash:expenses.tillTag")}</Badge> : null}
            </span>
          ),
        }),
        col.accessor("reference", {
          header: () => t("cash:expenses.columns.reference"),
          enableSorting: false,
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("note", {
          header: () => t("cash:expenses.columns.note"),
          enableSorting: false,
          cell: (c) => <span title={c.getValue() ?? undefined}>{c.getValue()}</span>,
        }),
        col.display({
          id: "status",
          header: () => t("cash:expenses.columns.status"),
          cell: ({ row }) => {
            const e = row.original;
            if (e.reversalOf) return <Badge>{t("cash:expenses.reversal")}</Badge>;
            if (e.reversed) return <Badge tone="warn">{t("cash:expenses.reversed")}</Badge>;
            return records ? (
              <Button variant="ghost" onClick={() => setReversing(e)}>
                <Undo2 aria-hidden />
                {t("cash:expenses.reverse")}
              </Button>
            ) : null;
          },
        }),
      ]),
    [t, lang, records],
  );

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <DateRangePicker value={range} onChange={setRange} />
        {records ? (
          <Button variant="primary" onClick={() => setCreating(true)}>
            <Plus aria-hidden />
            {t("cash:expenses.new")}
          </Button>
        ) : null}
      </div>
      <Card>
        <h2 className="text-xl font-bold">{t("cash:expenses.totals")}</h2>
        {(totals.data ?? []).length === 0 ? (
          <p className="text-ink-2">{t("cash:expenses.totalsEmpty")}</p>
        ) : (
          <ul
            aria-label={t("cash:expenses.totals")}
            className="grid gap-x-6 gap-y-1 sm:grid-cols-2 lg:grid-cols-3"
          >
            {(totals.data ?? []).map((c) => (
              <li
                key={c.categoryId}
                className="flex items-center justify-between gap-3 border-b border-line py-1"
              >
                <span>{localizedName(c, lang)}</span>
                <Amount value={c.total} className="font-semibold" />
              </li>
            ))}
          </ul>
        )}
      </Card>
      <TableToolbar
        search={search}
        onSearch={setSearch}
        exportTarget={exportTarget}
        filters={
          <Field id="expenses-category" label={t("cash:expense.category")}>
            <NativeSelect
              id="expenses-category"
              className="w-auto"
              value={categoryId}
              onChange={(event) => setCategoryId(event.target.value)}
            >
              <option value="">{t("cash:expenses.allCategories")}</option>
              {categories.map((c) => (
                <option key={c.id} value={c.id}>
                  {localizedName(c, lang)}
                </option>
              ))}
            </NativeSelect>
          </Field>
        }
      />
      {loaded.truncated ? <TruncatedNotice rows={LIST_LIMIT} /> : null}
      <DataTable
        ariaLabel={t("cash:expenses.title")}
        data={rows}
        columns={columns}
        sortable={!loaded.truncated}
        getRowId={(e) => e.id}
        rowHeight={52}
        empty={expenses.isPending ? t("common:status.loading") : t("cash:expenses.empty")}
      />
      {creating ? <ExpenseDialog onClose={() => setCreating(false)} /> : null}
      {reversing ? <ReverseDialog expense={reversing} onClose={() => setReversing(null)} /> : null}
    </div>
  );
}

/** A negative expense cancelling `expense`, with the reason (required by the backend too). */
function ReverseDialog({ expense, onClose }: { expense: Expense; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const reverse = useReverseExpense();
  const [note, setNote] = useState("");
  const [missing, setMissing] = useState(false);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (note.trim() === "") {
      setMissing(true);
      return;
    }
    reverse.mutate(
      { id: expense.id, note: note.trim() },
      {
        onSuccess: () => {
          toast.success(t("cash:expenses.reversedDone"));
          onClose();
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("cash:expenses.reverse")}
      description={t("cash:expenses.reverseBody", {
        amount: formatDZD(expense.amount, lang),
        category: localizedName(expense, lang),
      })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="expense-reverse" variant="danger" disabled={reverse.isPending}>
            {t("cash:expenses.reverse")}
          </Button>
        </>
      }
    >
      <form id="expense-reverse" noValidate onSubmit={submit} className="flex flex-col gap-4">
        {expense.fromTill ? <p className="text-warn">{t("cash:expenses.reverseTill")}</p> : null}
        <Field
          id="expense-reverse-note"
          label={t("cash:expenses.reverseNote")}
          error={missing ? t("validation:required") : undefined}
        >
          <Input
            id="expense-reverse-note"
            dir="auto"
            maxLength={200}
            aria-invalid={missing}
            value={note}
            onChange={(event) => {
              setNote(event.target.value);
              setMissing(false);
            }}
          />
        </Field>
        {reverse.error ? (
          <p role="alert" className="text-danger">
            {errorText(reverse.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
