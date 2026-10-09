import type { TFunction } from "i18next";
import { Printer } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { type Columns, DataTable, columnHelper } from "@/components/data-table/data-table";
import { ExportMenu } from "@/components/data-table/export-menu";
import { DateRangePicker } from "@/components/date-range";
import { LanguageToggle } from "@/components/language-toggle";
import { StatTile } from "@/components/stat-tile";
import { Button } from "@/components/ui/button";
import { type DateRange, dayToMs, isDay, rangeOf, today } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import { usePrintStatement, useStatement } from "@/lib/ipc/credit";
import type { Customer, LedgerEntry } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { StatementA4Actions } from "./StatementA4Actions";

const col = columnHelper<LedgerEntry>();

/** The account's entries as the notebook reads them: the day, what happened (and how a versement
 * was paid), the document or receipt number, the signed amount and the balance after it (the
 * backend's running balance). */
export function ledgerColumns(t: TFunction, lang: Lang): Columns<LedgerEntry> {
  return col.columns([
    col.accessor("businessDate", {
      header: () => t("customers:detail.columns.date"),
      cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
    }),
    col.accessor("kind", {
      header: () => t("customers:detail.columns.kind"),
      enableSorting: false,
      cell: ({ row }) => {
        const { kind, method } = row.original;
        const how = kind === "payment" && method ? ` · ${t(`payments:tenders.${method}`)}` : "";
        return `${t(`customers:kinds.${kind}`)}${how}`;
      },
    }),
    col.accessor((e) => e.refNumber ?? e.receiptNumber ?? "", {
      id: "number",
      header: () => t("customers:detail.columns.number"),
      enableSorting: false,
      cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
    }),
    col.accessor("amount", {
      header: () => t("customers:detail.columns.amount"),
      enableSorting: false,
      meta: { align: "end" },
      cell: (c) => <Amount value={c.getValue()} />,
    }),
    col.accessor("balanceAfter", {
      header: () => t("customers:detail.columns.balance"),
      enableSorting: false,
      meta: { align: "end" },
      cell: (c) => <Amount value={c.getValue()} className="font-semibold" />,
    }),
  ]);
}

/**
 * RPT-037, CRD-005: the statement of a period, the balance at its start, the debits and credits
 * and the balance at its end, all from `credit_statement`; printed on the receipt printer in the
 * chosen language, or on A4 and as a PDF in the same language (M3). Excel, CSV and PDF export the
 * account's activity over the period (`credit_activity` for this customer).
 */
export function StatementPanel({ customer }: { customer: Customer }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [range, setRange] = useState<DateRange>(() => rangeOf("this_month", today()));
  const [printLang, setPrintLang] = useState<Lang>(customer.lang ?? lang);
  const complete = isDay(range.from) && isDay(range.to);
  const valid = complete && range.from <= range.to;
  const statement = useStatement(valid ? customer.id : null, range.from, range.to);
  const print = usePrintStatement();
  const data = valid ? statement.data : undefined;
  const columns = useMemo(() => ledgerColumns(t, lang), [t, lang]);
  return (
    <section className="flex flex-col gap-4" aria-labelledby="statement-title">
      <h2 id="statement-title" className="text-xl font-bold">
        {t("customers:statement.title")}
      </h2>
      <DateRangePicker
        value={range}
        onChange={setRange}
        error={complete && !valid ? t("validation:rangeOrder") : undefined}
      />
      {valid && statement.error ? (
        <p role="alert" className="text-danger">
          {errorText(statement.error)}
        </p>
      ) : null}
      {data ? (
        <>
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <StatTile label={t("customers:statement.opening")} value={<Amount value={data.opening} />} />
            <StatTile label={t("customers:statement.debits")} value={<Amount value={data.debits} />} />
            <StatTile label={t("customers:statement.credits")} value={<Amount value={data.credits} />} />
            <StatTile label={t("customers:statement.closing")} value={<Amount value={data.closing} />} />
          </div>
          <DataTable
            ariaLabel={t("customers:statement.title")}
            data={data.entries}
            columns={columns}
            getRowId={(e) => e.id}
            empty={t("customers:statement.empty")}
          />
        </>
      ) : null}
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="flex flex-wrap items-center gap-3">
          <span className="font-semibold">{t("customers:statement.language")}</span>
          <LanguageToggle value={printLang} onChange={setPrintLang} />
          <Button
            variant="secondary"
            disabled={!valid || print.isPending}
            onClick={() =>
              print.mutate(
                { customerId: customer.id, from: range.from, to: range.to, lang: printLang },
                {
                  onSuccess: (outcome) =>
                    toast.success(t("customers:statement.printed", { printer: outcome.printer })),
                  onError: (error) => toast.error(errorText(error)),
                },
              )
            }
          >
            <Printer aria-hidden />
            {t("customers:statement.print")}
          </Button>
          <StatementA4Actions
            side="customer"
            partyId={customer.id}
            from={range.from}
            to={range.to}
            lang={printLang}
          />
        </div>
        <ExportMenu
          target={
            valid
              ? {
                  kind: "report",
                  request: {
                    kind: "credit_activity",
                    from: range.from,
                    to: range.to,
                    customerId: customer.id,
                  },
                }
              : null
          }
        />
      </div>
      <p className="text-sm text-ink-2">{t("customers:statement.exportHint")}</p>
    </section>
  );
}
