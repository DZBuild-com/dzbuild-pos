import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { dayToMs } from "@/lib/dates";
import { formatDate, formatDZD } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { toAppError } from "@/lib/ipc/client";
import { usePaySupplier, useSupplierOpenItems } from "@/lib/ipc/suppliers";
import type { LedgerEntry, PaymentMethod, PaymentNow, Supplier } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** How a supplier is paid (never on an account: credit, store credit). */
const METHODS: readonly PaymentMethod[] = ["cash", "cheque", "transfer", "card_cib", "edahabia", "baridimob"];

/** A payment as typed. */
export interface PaymentDraft {
  amount: string;
  method: PaymentMethod;
  reference: string;
  chequeBank: string;
  chequeDate: string;
  /** Cash taken out of this terminal's drawer (a `paid_out` movement). */
  fromTill: boolean;
}

export function emptyPayment(): PaymentDraft {
  return { amount: "", method: "cash", reference: "", chequeBank: "", chequeDate: "", fromTill: false };
}

const text = (value: string) => (value.trim() === "" ? null : value.trim());

/** The payment as the backend takes it; `null` while the amount is not a positive amount. */
export function toPaymentNow(draft: PaymentDraft): PaymentNow | null {
  const amount = parseAmount(draft.amount);
  if (amount === null || amount <= 0) return null;
  return {
    amount,
    method: draft.method,
    reference: draft.method === "cash" ? null : text(draft.reference),
    chequeBank: draft.method === "cheque" ? text(draft.chequeBank) : null,
    chequeDate: draft.method === "cheque" ? text(draft.chequeDate) : null,
    fromTill: draft.method === "cash" && draft.fromTill,
  };
}

/**
 * PUR-009/010: the amount, the method (with the cheque's or transfer's details) and, for cash, the
 * "from the drawer" switch, shown only to who may take cash out (`cash.paid_in_out`). `errors`
 * holds the backend's refusals by field (`amount`, `reference`, `method`).
 */
export function PaymentFields({
  idPrefix,
  value,
  onChange,
  errors,
}: {
  idPrefix: string;
  value: PaymentDraft;
  onChange: (value: PaymentDraft) => void;
  errors: Partial<Record<"amount" | "reference" | "method", string>>;
}) {
  const { t } = useTranslation();
  const mayTakeCash = useSession().data?.permissions.includes("cash.paid_in_out") ?? false;
  const set = (change: Partial<PaymentDraft>) => onChange({ ...value, ...change });
  return (
    <div className="flex flex-col gap-4">
      <Field id={`${idPrefix}-amount`} label={t("purchases:pay.amount")} error={errors.amount}>
        <NumericInput
          id={`${idPrefix}-amount`}
          value={value.amount}
          aria-invalid={Boolean(errors.amount)}
          onValueChange={(amount) => set({ amount })}
          className="text-xl font-bold"
        />
      </Field>
      <div className="flex flex-col gap-2">
        <span id={`${idPrefix}-method`} className="font-semibold">
          {t("purchases:pay.method")}
        </span>
        <div role="radiogroup" aria-labelledby={`${idPrefix}-method`} className="grid grid-cols-3 gap-2">
          {METHODS.map((method) => (
            <button
              key={method}
              type="button"
              role="radio"
              aria-checked={value.method === method}
              onClick={() => set({ method })}
              className={cn(
                "min-h-14 rounded-control border px-2 font-semibold",
                value.method === method ? "border-brand bg-brand-soft" : "border-line bg-surface-2",
              )}
            >
              {t(`payments:tenders.${method}`)}
            </button>
          ))}
        </div>
        {errors.method ? (
          <p role="alert" className="text-sm text-danger">
            {errors.method}
          </p>
        ) : null}
      </div>
      {value.method === "cash" ? (
        mayTakeCash ? (
          <label className="flex min-h-11 items-center gap-3 font-semibold">
            <Checkbox checked={value.fromTill} onCheckedChange={(v) => set({ fromTill: v === true })} />
            {t("purchases:pay.fromTill")}
          </label>
        ) : null
      ) : (
        <div className="grid gap-3 sm:grid-cols-3">
          <Field
            id={`${idPrefix}-reference`}
            label={t(`payments:reference.${value.method}`, { method: t(`payments:tenders.${value.method}`) })}
            error={errors.reference}
          >
            <GuardedInput
              id={`${idPrefix}-reference`}
              dir="ltr"
              maxLength={40}
              aria-invalid={Boolean(errors.reference)}
              value={value.reference}
              onValueChange={(reference) => set({ reference })}
            />
          </Field>
          {value.method === "cheque" ? (
            <>
              <Field id={`${idPrefix}-bank`} label={t("payments:cheque.bank")}>
                <GuardedInput
                  id={`${idPrefix}-bank`}
                  maxLength={48}
                  value={value.chequeBank}
                  onValueChange={(chequeBank) => set({ chequeBank })}
                />
              </Field>
              <Field id={`${idPrefix}-cheque-date`} label={t("payments:cheque.date")}>
                <Input
                  id={`${idPrefix}-cheque-date`}
                  type="date"
                  dir="ltr"
                  value={value.chequeDate}
                  onChange={(event) => set({ chequeDate: event.target.value })}
                />
              </Field>
            </>
          ) : null}
        </div>
      )}
    </div>
  );
}

/** The backend refusal of a payment field (`amount`, `reference`, `method`), or `null`. */
export function paymentFieldOf(error: unknown): "amount" | "reference" | "method" | null {
  const field = toAppError(error).field ?? "";
  const bare = field.replace(/^payment\./, "");
  return bare === "amount" || bare === "reference" || bare === "method" ? bare : null;
}

/** A supplier invoice still open, which a payment may settle. */
function OpenInvoice({
  item,
  checked,
  onToggle,
}: {
  item: LedgerEntry;
  checked: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  return (
    <label className="flex min-h-11 items-center gap-3 rounded-control px-2 hover:bg-surface-2">
      <Checkbox checked={checked} onCheckedChange={onToggle} />
      <span className="flex flex-1 flex-wrap items-center gap-x-3">
        <bdi dir="ltr" className="tabular-nums">
          {formatDate(dayToMs(item.businessDate), lang)}
        </bdi>
        {item.refNumber ? (
          <bdi dir="ltr" className="font-semibold">
            {item.refNumber}
          </bdi>
        ) : (
          <span className="font-semibold">{t(`customers:kinds.${item.kind}`)}</span>
        )}
      </span>
      <Amount value={item.open} />
    </label>
  );
}

/**
 * PUR-009/010: a payment to a supplier (partial or not), by any method but an account; cash may
 * come out of the drawer. The supplier invoices are settled oldest first unless some are chosen.
 * The balance after it is the backend's.
 */
export function PaySupplierDialog({
  supplier,
  open,
  onClose,
}: {
  supplier: Supplier;
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [payment, setPayment] = useState<PaymentDraft>(emptyPayment);
  const [chooseInvoices, setChooseInvoices] = useState(false);
  const [chosen, setChosen] = useState<string[]>([]);
  const [note, setNote] = useState("");
  const [invalid, setInvalid] = useState(false);
  const openItems = useSupplierOpenItems(open ? supplier.id : null);
  const invoices = openItems.data ?? [];
  const pay = usePaySupplier();
  const refusedField = pay.error ? paymentFieldOf(pay.error) : null;

  const submit = () => {
    const now = toPaymentNow(payment);
    setInvalid(now === null);
    if (!now) return;
    pay.mutate(
      {
        supplierId: supplier.id,
        ...now,
        allocateTo: chooseInvoices ? invoices.filter((i) => chosen.includes(i.id)).map((i) => i.id) : [],
        note: text(note),
      },
      {
        onSuccess: (entry) => {
          toast.success(
            t("purchases:pay.done", {
              amount: formatDZD(now.amount, lang),
              balance: formatDZD(entry.balanceAfter, lang),
            }),
          );
          onClose();
        },
      },
    );
  };
  const errors = {
    amount: invalid
      ? t("validation:amountFormat")
      : refusedField === "amount"
        ? errorText(pay.error)
        : undefined,
    reference: refusedField === "reference" ? errorText(pay.error) : undefined,
    method: refusedField === "method" ? errorText(pay.error) : undefined,
  };
  const toggle = (id: string) =>
    setChosen((ids) => (ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id]));

  return (
    <Dialog
      open={open}
      size="lg"
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("purchases:pay.title", { name: supplier.name })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={pay.isPending} onClick={submit}>
            {t("purchases:pay.confirm")}
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
        <p className="flex flex-wrap items-center justify-between gap-2 rounded-card bg-surface-2 p-4">
          <span>{t("purchases:supplier.balance")}</span>
          <Amount value={supplier.balance} className="text-xl font-bold" />
        </p>
        <PaymentFields
          idPrefix="pay"
          value={payment}
          onChange={(value) => {
            setPayment(value);
            setInvalid(false);
          }}
          errors={errors}
        />
        {invoices.length > 1 ? (
          <fieldset className="flex flex-col gap-2">
            <legend className="sr-only">{t("purchases:pay.openInvoices")}</legend>
            <label className="flex min-h-11 items-center gap-3">
              <input
                type="radio"
                name="pay-allocation"
                className="size-5 accent-brand"
                checked={!chooseInvoices}
                onChange={() => setChooseInvoices(false)}
              />
              {t("purchases:pay.allocateOldest")}
            </label>
            <label className="flex min-h-11 items-center gap-3">
              <input
                type="radio"
                name="pay-allocation"
                className="size-5 accent-brand"
                checked={chooseInvoices}
                onChange={() => setChooseInvoices(true)}
              />
              {t("purchases:pay.allocateChosen")}
            </label>
            {chooseInvoices ? (
              <div className="flex flex-col gap-1 rounded-card border border-line p-2">
                {invoices.map((item) => (
                  <OpenInvoice
                    key={item.id}
                    item={item}
                    checked={chosen.includes(item.id)}
                    onToggle={() => toggle(item.id)}
                  />
                ))}
              </div>
            ) : null}
          </fieldset>
        ) : null}
        <Field id="pay-note" label={t("purchases:pay.note")} optional={t("customers:form.optional")}>
          <Input
            id="pay-note"
            dir="auto"
            maxLength={200}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </Field>
        {pay.error && refusedField === null ? (
          <p role="alert" className="text-danger">
            {errorText(pay.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
