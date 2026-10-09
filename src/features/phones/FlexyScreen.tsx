import { FileBarChart, ShoppingCart } from "lucide-react";
import { type FormEvent, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { FlexyOperatorsSetting } from "@/features/packs/setting-cards";
import { Link } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { DateRangePicker } from "@/components/date-range";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { type DateRange, dayToMs, rangeOf, today } from "@/lib/dates";
import { formatDate, formatDZD } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import { useSession } from "@/lib/ipc/auth";
import { useFlexyBalances, useFlexyEntries, useFlexyOperators, useFlexyPurchase } from "@/lib/ipc/phones";
import type { ExportTarget, FlexyOperator, FloatEntry, Permission } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** Under 5 000 DA of credit an operator's card turns red: time to buy float. */
const LOW_FLOAT = 500_000;

const col = columnHelper<FloatEntry>();

/**
 * VRT-002, the cash page's Flexy tab: the float left per operator, "Acheter du stock" (the credit
 * bought from the distributor, what was paid, from the till or not; on a till it is recorded on
 * the main PC), and the float's movements over a period. The commission per operator is the
 * backend's, in the flexy sales report (FlexySales) this tab links to and exports.
 */
export function FlexyScreen() {
  const { t } = useTranslation();
  const lang = useLang();
  const session = useSession();
  const operators = useFlexyOperators();
  const balances = useFlexyBalances();
  const [buying, setBuying] = useState(false);
  const can = (p: Permission) => session.data?.permissions.includes(p) ?? false;
  const list = operators.data ?? [];
  const balanceOf = (o: FlexyOperator) => balances.data?.find(([code]) => code === o.code)?.[1] ?? o.balance;

  return (
    <div className="flex flex-col gap-6">
      {/* final review: each operator's product and commission, set by the owner */}
      {can("settings.company") ? <FlexyOperatorsSetting /> : null}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-2xl font-bold">{t("phones:flexy.title")}</h2>
        {can("cash.paid_in_out") ? (
          <Button variant="primary" disabled={list.length === 0} onClick={() => setBuying(true)}>
            <ShoppingCart aria-hidden />
            {t("phones:flexy.buy")}
          </Button>
        ) : null}
      </div>
      <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {list.map((o) => {
          const balance = balanceOf(o);
          const low = balance < LOW_FLOAT;
          const name = localizedName(o, lang);
          return (
            <li
              key={o.code}
              className={cn(
                "flex flex-col gap-1 rounded-card border border-line p-4",
                low ? "bg-danger-soft" : "bg-surface",
              )}
            >
              <span className="flex items-center justify-between gap-2 font-semibold">
                {t("phones:flexy.balance", { operator: name })}
                {low ? <Badge tone="danger">{t("phones:flexy.low")}</Badge> : null}
              </span>
              <Amount value={balance} className="text-2xl font-bold" />
            </li>
          );
        })}
      </ul>
      {can("reports.cash") ? <Entries operators={list} /> : null}
      {buying ? <PurchaseDialog operators={list} onClose={() => setBuying(false)} /> : null}
    </div>
  );
}

/** The float's ledger over a period, one operator or all (`reports.cash`). */
function Entries({ operators }: { operators: FlexyOperator[] }) {
  const { t } = useTranslation();
  const lang = useLang();
  const [range, setRange] = useState<DateRange>(() => rangeOf("this_month", today()));
  const [operator, setOperator] = useState("");
  const [search, setSearch] = useState("");
  const entries = useFlexyEntries(operator === "" ? null : operator, range.from, range.to);
  const text = search.trim().toLowerCase();
  const rows = (entries.data ?? []).filter(
    (e) =>
      text === "" ||
      [nameOfIn(operators, e.operator, lang), e.phoneTail ?? ""].some((v) => v.toLowerCase().includes(text)),
  );
  const report = `/reports/flexy_sales?from=${range.from}&to=${range.to}`;
  const exportTarget: ExportTarget = {
    kind: "report",
    request: { kind: "flexy_sales", from: range.from, to: range.to, lang },
  };
  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("businessDate", {
          header: () => t("phones:flexy.columns.date"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        col.accessor((e) => nameOfIn(operators, e.operator, lang), {
          id: "operator",
          header: () => t("phones:flexy.operator"),
        }),
        col.accessor((e) => t(`phones:flexy.kinds.${e.kind}`), {
          id: "kind",
          header: () => t("phones:flexy.columns.kind"),
        }),
        col.accessor("amount", {
          header: () => t("phones:flexy.columns.amount"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("cost", {
          header: () => t("phones:flexy.columns.cost"),
          meta: { align: "end" },
          cell: (c) => (c.getValue() === 0 ? null : <Amount value={c.getValue()} />),
        }),
        col.accessor("phoneTail", {
          header: () => t("phones:flexy.columns.phone"),
          enableSorting: false,
          cell: (c) => (c.getValue() ? <bdi dir="ltr">…{c.getValue()}</bdi> : null),
        }),
      ]),
    [t, lang, operators],
  );

  return (
    <section aria-labelledby="flexy-entries" className="flex flex-col gap-4">
      <h2 id="flexy-entries" className="text-xl font-bold">
        {t("phones:flexy.entries")}
      </h2>
      <DateRangePicker value={range} onChange={setRange} />
      <Card className="flex-row flex-wrap items-center justify-between gap-3 p-4">
        <p className="text-ink-2">{t("phones:flexy.commissionHint")}</p>
        <Button asChild variant="outline">
          <Link to={report}>
            <FileBarChart aria-hidden />
            {t("phones:flexy.commission")}
          </Link>
        </Button>
      </Card>
      <TableToolbar
        search={search}
        onSearch={setSearch}
        exportTarget={exportTarget}
        filters={
          <Field id="flexy-operator" label={t("phones:flexy.operator")}>
            <NativeSelect
              id="flexy-operator"
              className="w-auto"
              value={operator}
              onChange={(event) => setOperator(event.target.value)}
            >
              <option value="">{t("phones:flexy.allOperators")}</option>
              {operators.map((o) => (
                <option key={o.code} value={o.code}>
                  {localizedName(o, lang)}
                </option>
              ))}
            </NativeSelect>
          </Field>
        }
      />
      <DataTable
        ariaLabel={t("phones:flexy.entries")}
        data={rows}
        columns={columns}
        getRowId={(e) => e.id}
        empty={entries.isPending ? t("common:status.loading") : t("phones:flexy.noEntries")}
      />
    </section>
  );
}

/** The operator's name in `lang` (its code when the data pack no longer lists it). */
function nameOfIn(operators: FlexyOperator[], code: string, lang: Lang): string {
  const found = operators.find((o) => o.code === code);
  return found ? localizedName(found, lang) : code;
}

/** Float bought from the distributor: the credit added, what it cost, paid from the drawer or not. */
function PurchaseDialog({ operators, onClose }: { operators: FlexyOperator[]; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const purchase = useFlexyPurchase();
  const [operator, setOperator] = useState("");
  const [amount, setAmount] = useState("");
  const [cost, setCost] = useState("");
  const [fromTill, setFromTill] = useState(false);
  const [note, setNote] = useState("");
  const [errors, setErrors] = useState<{ operator?: string; amount?: string; cost?: string }>({});
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const credit = parseAmount(amount);
    const paid = parseAmount(cost);
    const found = {
      operator: operator === "" ? t("validation:required") : undefined,
      amount: credit === null || credit <= 0 ? t("validation:amountFormat") : undefined,
      cost: paid === null || paid < 0 ? t("validation:amountFormat") : undefined,
    };
    setErrors(found);
    if (operator === "" || credit === null || credit <= 0 || paid === null || paid < 0) return;
    purchase.mutate(
      { operator, amount: credit, cost: paid, fromTill, note: note.trim() === "" ? null : note.trim() },
      {
        onSuccess: (entry) => {
          const chosen = operators.find((o) => o.code === entry.operator);
          toast.success(
            t("phones:flexy.bought", {
              operator: chosen ? localizedName(chosen, lang) : entry.operator,
              amount: formatDZD(entry.amount, lang),
            }),
          );
          onClose();
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("phones:flexy.buy")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="flexy-purchase" variant="primary" disabled={purchase.isPending}>
            {t("phones:flexy.save")}
          </Button>
        </>
      }
    >
      <form id="flexy-purchase" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <RadioCards
          legend={t("phones:flexy.operator")}
          name="flexy-operator"
          value={operator}
          onChange={setOperator}
          options={operators.map((o) => ({ value: o.code, label: localizedName(o, lang) }))}
        />
        {errors.operator ? (
          <p role="alert" className="text-sm text-danger">
            {errors.operator}
          </p>
        ) : null}
        <Field id="flexy-amount" label={t("phones:flexy.amount")} error={errors.amount}>
          <NumericInput id="flexy-amount" value={amount} onValueChange={setAmount} />
        </Field>
        <Field id="flexy-cost" label={t("phones:flexy.cost")} error={errors.cost}>
          <NumericInput id="flexy-cost" value={cost} onValueChange={setCost} />
        </Field>
        <label className="flex min-h-11 items-center gap-3 font-semibold">
          <Checkbox checked={fromTill} onCheckedChange={(v) => setFromTill(v === true)} />
          {t("phones:flexy.fromTill")}
        </label>
        <Field id="flexy-note" label={t("phones:flexy.note")}>
          <Input
            id="flexy-note"
            dir="auto"
            maxLength={200}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </Field>
        {purchase.error ? (
          <p role="alert" className="text-danger">
            {errorText(purchase.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
