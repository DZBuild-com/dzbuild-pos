import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { GuardedInput } from "@/components/numeric-input";
import { StatTile } from "@/components/stat-tile";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { toAppError } from "@/lib/ipc/client";
import { usePurchases, useRecordInvoice } from "@/lib/ipc/purchases";
import type { InvoiceDone, Supplier } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { PaymentFields, emptyPayment, paymentFieldOf, toPaymentNow } from "./PaySupplierDialog";

/** `receiptIds.1` → 1: the chosen receipt a refusal names (`receipt_already_invoiced`). */
function refusedReceipt(field: string | null | undefined): number | null {
  const match = /^receiptIds\.(\d+)$/.exec(field ?? "");
  return match ? Number(match[1]) : null;
}

/**
 * PUR-005/008: the supplier's invoice (`FF…`, their own number kept) over one or more goods
 * receipts. The totals are the backend's: each receipt's HT, TVA and TTC, then the invoice's once
 * recorded. A receipt already invoiced, or an invoice number already recorded for this supplier,
 * is refused where it was typed. The invoice may be paid at once.
 */
export function InvoiceDialog({
  supplier,
  receiptIds = [],
  open,
  onClose,
}: {
  supplier: Supplier;
  /** Receipts ticked at the start (the receipt just validated). */
  receiptIds?: string[];
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const receipts = usePurchases({ docType: "goods_receipt", supplierId: supplier.id });
  const [chosen, setChosen] = useState<string[]>(receiptIds);
  const [number, setNumber] = useState("");
  const [note, setNote] = useState("");
  const [payNow, setPayNow] = useState(false);
  const [payment, setPayment] = useState(emptyPayment);
  const [problems, setProblems] = useState<{ number?: string; receipts?: string; amount?: string }>({});
  const [done, setDone] = useState<InvoiceDone | null>(null);
  // What the last attempt sent: a refusal's `receiptIds.N` names its N-th receipt.
  const [sentIds, setSentIds] = useState<string[]>([]);
  const record = useRecordInvoice();
  const refusal = record.error ? toAppError(record.error) : null;
  const refusedField = refusal?.field ?? null;
  const refusedPayment = record.error ? paymentFieldOf(record.error) : null;
  const refusedAt = refusedReceipt(refusedField);
  const list = receipts.data ?? [];

  const submit = () => {
    const found: typeof problems = {};
    if (number.trim() === "") found.number = t("validation:required");
    if (chosen.length === 0) found.receipts = t("purchases:invoice.noReceipt");
    const now = payNow ? toPaymentNow(payment) : null;
    if (payNow && now === null) found.amount = t("validation:amountFormat");
    setProblems(found);
    if (Object.keys(found).length > 0) return;
    // In the list's order (a receipt just validated may not be listed yet: it goes last).
    const ids = [
      ...list.filter((r) => chosen.includes(r.id)).map((r) => r.id),
      ...chosen.filter((id) => !list.some((r) => r.id === id)),
    ];
    setSentIds(ids);
    record.mutate(
      {
        supplierId: supplier.id,
        supplierNumber: number.trim(),
        receiptIds: ids,
        lines: [],
        note: note.trim() === "" ? null : note.trim(),
        payment: now,
      },
      {
        onSuccess: (result) => {
          toast.success(t("purchases:invoice.done", { number: result.invoice.number ?? "" }));
          setDone(result);
        },
      },
    );
  };
  const toggle = (id: string) =>
    setChosen((ids) => (ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id]));
  const refusedId = refusedAt !== null ? sentIds[refusedAt] : undefined;

  if (done) {
    const invoice = done.invoice;
    return (
      <Dialog
        open={open}
        onOpenChange={(o) => (o ? undefined : onClose())}
        title={t("purchases:invoice.doneTitle", { number: invoice.number ?? "" })}
        footer={
          <Button variant="primary" onClick={onClose}>
            {t("common:actions.done")}
          </Button>
        }
      >
        <div className="grid gap-3 sm:grid-cols-3">
          <StatTile label={t("purchases:columns.totalHt")} value={<Amount value={invoice.subtotalHt} />} />
          <StatTile label={t("purchases:columns.tax")} value={<Amount value={invoice.taxTotal} />} />
          <StatTile label={t("purchases:columns.totalTtc")} value={<Amount value={invoice.totalTtc} />} />
        </div>
        {done.payment ? (
          <p className="flex flex-wrap items-center gap-2">
            {t("purchases:invoice.paid")} <Amount value={-done.payment.amount} className="font-semibold" />
          </p>
        ) : null}
        <p className="flex flex-wrap items-center gap-2">
          {t("purchases:supplier.balance")}{" "}
          <Amount value={done.payment?.balanceAfter ?? done.entry.balanceAfter} className="font-semibold" />
        </p>
      </Dialog>
    );
  }

  return (
    <Dialog
      open={open}
      size="lg"
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("purchases:invoice.title", { name: supplier.name })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={record.isPending} onClick={submit}>
            {t("purchases:invoice.confirm")}
          </Button>
        </>
      }
    >
      <form
        noValidate
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <div className="grid gap-4 sm:grid-cols-2">
          <Field
            id="invoice-number"
            label={t("purchases:invoice.number")}
            hint={t("purchases:invoice.dateHint")}
            error={
              problems.number ?? (refusedField === "supplierNumber" ? errorText(record.error) : undefined)
            }
          >
            <GuardedInput
              id="invoice-number"
              dir="ltr"
              autoFocus
              maxLength={48}
              aria-invalid={Boolean(problems.number) || refusedField === "supplierNumber"}
              value={number}
              onValueChange={setNumber}
            />
          </Field>
          <Field
            id="invoice-note"
            label={t("purchases:invoice.note")}
            optional={t("customers:form.optional")}
          >
            <Input
              id="invoice-note"
              dir="auto"
              maxLength={500}
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </Field>
        </div>
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 font-semibold">{t("purchases:invoice.receipts")}</legend>
          {receipts.isPending ? <p className="text-ink-2">{t("common:status.loading")}</p> : null}
          {receipts.isSuccess && list.length === 0 ? (
            <p className="text-ink-2">{t("purchases:invoice.noReceipts")}</p>
          ) : null}
          <ul className="flex flex-col gap-1">
            {list.map((r) => (
              <li key={r.id} className="flex flex-col rounded-control px-2 hover:bg-surface-2">
                <label className="flex min-h-11 flex-wrap items-center gap-x-4 gap-y-1">
                  <Checkbox checked={chosen.includes(r.id)} onCheckedChange={() => toggle(r.id)} />
                  <bdi dir="ltr" className="font-semibold">
                    {r.number}
                  </bdi>
                  <bdi dir="ltr" className="text-ink-2 tabular-nums">
                    {formatDate(dayToMs(r.businessDate), lang)}
                  </bdi>
                  {r.supplierRef ? (
                    <span className="text-ink-2">
                      {t("purchases:columns.supplierRef")} <bdi dir="ltr">{r.supplierRef}</bdi>
                    </span>
                  ) : null}
                  <span className="ms-auto flex flex-wrap gap-x-4">
                    <span>
                      {t("purchases:columns.totalHt")} <Amount value={r.totalHt} />
                    </span>
                    <span>
                      {t("purchases:columns.tax")} <Amount value={r.taxTotal} />
                    </span>
                    <span className="font-semibold">
                      {t("purchases:columns.totalTtc")} <Amount value={r.totalTtc} />
                    </span>
                  </span>
                </label>
                {refusedId === r.id ? (
                  <p role="alert" className="pb-2 text-sm text-danger">
                    {errorText(record.error)}
                  </p>
                ) : null}
              </li>
            ))}
          </ul>
          {problems.receipts ? (
            <p role="alert" className="text-sm text-danger">
              {problems.receipts}
            </p>
          ) : null}
        </fieldset>
        <label className="flex min-h-11 items-center gap-3 font-semibold">
          <Checkbox checked={payNow} onCheckedChange={(v) => setPayNow(v === true)} />
          {t("purchases:invoice.payNow")}
        </label>
        {payNow ? (
          <PaymentFields
            idPrefix="invoice-pay"
            value={payment}
            onChange={(value) => {
              setPayment(value);
              setProblems((p) => ({ ...p, amount: undefined }));
            }}
            errors={{
              amount: problems.amount ?? (refusedPayment === "amount" ? errorText(record.error) : undefined),
              reference: refusedPayment === "reference" ? errorText(record.error) : undefined,
              method: refusedPayment === "method" ? errorText(record.error) : undefined,
            }}
          />
        ) : null}
        {record.error &&
        refusedField !== "supplierNumber" &&
        refusedAt === null &&
        refusedPayment === null ? (
          <p role="alert" className="text-danger">
            {errorText(record.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
