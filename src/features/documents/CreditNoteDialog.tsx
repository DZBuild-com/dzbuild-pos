import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { useApproval } from "@/components/manager-approval";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { isApprovalCancelled } from "@/lib/approval";
import { useCreateCreditNote, useCreditQuote, useCreditable } from "@/lib/ipc/billing";
import type {
  CreditKind,
  CreditLineInput,
  CreditNoteInput,
  CreditReason,
  CreditableLine,
  Document,
  PaymentMethod,
} from "@/lib/ipc/types";
import { formatQty, parseAmount, parseQtyOrZero } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { PrintDialog } from "./PrintDialog";

const KINDS: CreditKind[] = ["goods", "financial"];
const REASONS: CreditReason[] = [
  "defective",
  "wrong_item",
  "price_adjustment",
  "commercial_gesture",
  "cancellation",
  "other",
];
/** The refunds `create_credit_note` takes; `credit` (back to the account) needs the invoice's
 * customer. */
const REFUNDS: PaymentMethod[] = ["cash", "card_cib", "edahabia", "baridimob", "cheque", "transfer"];

/** A cheque's number, a transfer's reference: the backend refuses the refund without it. */
const needsReference = (method: PaymentMethod) => method === "cheque" || method === "transfer";

/** The quantity typed on a line in milli-units (0 when empty), `null` when malformed. */
function typedQty(row: CreditableLine, text: string): number | null {
  if (text.trim() === "") return 0;
  return parseQtyOrZero(text, row.line.unitCode === "pc" ? 0 : 3);
}

/**
 * DOC-006/007: the credit note (avoir) that corrects a validated invoice — the only correction
 * path of an invoice. Goods coming back (a quantity per line, at most what earlier credit notes
 * left, back to stock or not) or a financial credit (an amount off the TTC), a reason and a
 * refund. Every figure is the backend's `documents_credit_quote`; a quantity above what is left
 * never reaches it. Once created, the credit note's print dialog opens.
 */
export function CreditNoteDialog({
  invoice,
  open,
  onClose,
}: {
  invoice: Document;
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const creditable = useCreditable(open ? invoice.id : null);
  const create = useCreateCreditNote();
  const approval = useApproval();
  const [kind, setKind] = useState<CreditKind>("goods");
  const [qty, setQty] = useState<Record<string, string>>({});
  const [restock, setRestock] = useState<Record<string, boolean>>({});
  const [amount, setAmount] = useState("");
  const [reason, setReason] = useState<CreditReason>("defective");
  const [refund, setRefund] = useState<PaymentMethod>("cash");
  const [reference, setReference] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState<unknown>(null);
  const [created, setCreated] = useState<Document | null>(null);

  const refunds: PaymentMethod[] = invoice.customerId !== null ? [...REFUNDS, "credit"] : REFUNDS;
  // The lines still holding something, with what was typed on each.
  const rows = (creditable.data ?? [])
    .filter((r) => r.remaining > 0)
    .map((r) => ({ row: r, milli: typedQty(r, qty[r.line.id] ?? "") }));
  const rowError = (row: CreditableLine, milli: number | null): string | undefined => {
    if (milli === null) return t("validation:qtyFormat");
    if (milli > row.remaining) return t("billing:credit.atMost", { qty: formatQty(row.remaining, lang) });
    return undefined;
  };
  const goodsValid = rows.every(({ row, milli }) => rowError(row, milli) === undefined);
  const lines: CreditLineInput[] = rows.flatMap(({ row, milli }) =>
    milli !== null && milli > 0
      ? [{ originLineId: row.line.id, qty: milli, restock: restock[row.line.id] ?? true }]
      : [],
  );
  const money = parseAmount(amount);
  const complete = kind === "goods" ? goodsValid && lines.length > 0 : money !== null && money > 0;
  const trimmed = reference.trim();
  const input: CreditNoteInput | null = complete
    ? {
        invoiceId: invoice.id,
        kind,
        lines: kind === "goods" ? lines : [],
        amount: kind === "financial" ? money : null,
        reason,
        refundMethod: refund,
        refundReference: needsReference(refund) && trimmed !== "" ? trimmed : null,
        note: note.trim() === "" ? null : note.trim(),
        secondLang: null,
      }
    : null;
  // The figures do not depend on the reference or the note: typing them asks nothing.
  const quote = useCreditQuote(input && { ...input, refundReference: null, note: null });
  const ready = input !== null && (!needsReference(refund) || trimmed !== "") && !create.isPending;

  const submit = () => {
    if (!input) return;
    setError(null);
    approval
      .run(() => create.mutateAsync(input))
      .then(
        (done) => {
          toast.success(t("billing:credit.done", { number: done.document.number ?? "" }));
          if (done.invoiceFullyCredited) {
            toast.info(t("billing:credit.invoiceCancelled", { number: invoice.number ?? "" }));
          }
          setCreated(done.document);
        },
        (failure: unknown) => {
          if (!isApprovalCancelled(failure)) setError(failure);
        },
      );
  };

  if (!open) return null;
  if (created) {
    return <PrintDialog id={created.id} docType={created.docType} doc={created} open onClose={onClose} />;
  }
  // The form starts from the invoice's lines: it waits for them (or for the backend's refusal).
  if (creditable.isPending) return null;
  const figures = input ? quote.data : undefined;
  return (
    <>
      <Dialog
        open
        size="lg"
        onOpenChange={(o) => (o ? undefined : onClose())}
        title={t("billing:credit.title", { number: invoice.number ?? "" })}
        footer={
          <>
            <Button variant="outline" onClick={onClose}>
              {t("common:actions.cancel")}
            </Button>
            <Button variant="primary" disabled={!ready} onClick={submit}>
              {t("billing:credit.submit")}
            </Button>
          </>
        }
      >
        <RadioCards
          legend={t("billing:credit.kind")}
          name="credit-kind"
          value={kind}
          onChange={setKind}
          options={KINDS.map((k) => ({ value: k, label: t(`billing:credit.${k}`) }))}
        />
        {kind === "goods" ? (
          <ul className="flex flex-col gap-3">
            {rows.map(({ row, milli }) => {
              const id = `credit-qty-${row.line.id}`;
              const name = row.line.description;
              return (
                <li
                  key={row.line.id}
                  className="grid gap-3 rounded-card border border-line p-3 sm:grid-cols-2"
                >
                  <Field id={id} label={t("billing:credit.qtyFor", { name })} error={rowError(row, milli)}>
                    <NumericInput
                      id={id}
                      kind="qty"
                      value={qty[row.line.id] ?? ""}
                      onValueChange={(v) => setQty((all) => ({ ...all, [row.line.id]: v }))}
                    />
                  </Field>
                  {/* unticked, the goods go to the damaged-goods location */}
                  <label className="flex min-h-11 items-center gap-2 self-end font-semibold">
                    <Checkbox
                      checked={restock[row.line.id] ?? true}
                      onCheckedChange={(v) => setRestock((all) => ({ ...all, [row.line.id]: v === true }))}
                    />
                    {t("billing:credit.restock")}
                  </label>
                </li>
              );
            })}
          </ul>
        ) : (
          <Field
            id="credit-amount"
            label={t("billing:credit.amount")}
            error={amount.trim() !== "" && money === null ? t("validation:amountFormat") : undefined}
          >
            <NumericInput id="credit-amount" value={amount} onValueChange={setAmount} />
          </Field>
        )}
        <div className="grid gap-3 sm:grid-cols-2">
          <Field id="credit-reason" label={t("billing:credit.reason")}>
            <NativeSelect
              id="credit-reason"
              value={reason}
              onChange={(e) => setReason(e.target.value as CreditReason)}
            >
              {REASONS.map((r) => (
                <option key={r} value={r}>
                  {t(`billing:credit.reasons.${r}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
          <Field id="credit-refund" label={t("billing:credit.refund")}>
            <NativeSelect
              id="credit-refund"
              value={refund}
              onChange={(e) => setRefund(e.target.value as PaymentMethod)}
            >
              {refunds.map((m) => (
                <option key={m} value={m}>
                  {t(`payments:tenders.${m}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
        </div>
        {needsReference(refund) ? (
          <Field id="credit-reference" label={t("billing:credit.refundReference")}>
            <GuardedInput
              id="credit-reference"
              dir="ltr"
              maxLength={48}
              value={reference}
              onValueChange={setReference}
            />
          </Field>
        ) : null}
        <Field id="credit-note" label={t("billing:editor.note")}>
          <Input
            id="credit-note"
            dir="auto"
            maxLength={200}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </Field>
        {figures ? (
          <dl className="grid gap-x-6 gap-y-1 rounded-control bg-surface-2 p-3 sm:grid-cols-[1fr_auto]">
            <dt className="font-semibold">{t("billing:credit.quote.total")}</dt>
            <dd className="text-end">
              <Amount value={figures.totalTtc} className="text-lg font-bold" />
            </dd>
            {/* An IFU invoice never shows TVA (CTCA art. 64), nor does its credit note. */}
            {invoice.taxRegime === "reel" ? (
              <>
                <dt>{t("billing:credit.quote.tax")}</dt>
                <dd className="text-end">
                  <Amount value={figures.taxTotal} />
                </dd>
              </>
            ) : null}
            <dt>{t("billing:credit.quote.refund")}</dt>
            <dd className="text-end">
              <Amount value={figures.refund} />
            </dd>
            <dt>{t("billing:credit.quote.remaining")}</dt>
            <dd className="text-end">
              <Amount value={figures.invoiceRemaining} />
            </dd>
          </dl>
        ) : null}
        {input && quote.error ? (
          <p role="alert" className="text-danger">
            {errorText(quote.error)}
          </p>
        ) : null}
        {creditable.error ? (
          <p role="alert" className="text-danger">
            {errorText(creditable.error)}
          </p>
        ) : null}
        {error !== null ? (
          <p role="alert" className="text-danger">
            {errorText(error)}
          </p>
        ) : null}
      </Dialog>
      {approval.dialog}
    </>
  );
}
