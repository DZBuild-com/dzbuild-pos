import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { formatAmountInput, formatDZD } from "@/lib/format";
import { useCreditQuote } from "@/lib/ipc/credit";
import { useHardwareConfig } from "@/lib/ipc/hardware";
import { useCollectInstalment } from "@/lib/ipc/instalments";
import type { PaymentMethod, Plan, PlanPayment } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { PAY_METHODS, nextDue } from "./kinds";

const text = (value: string) => (value.trim() === "" ? null : value.trim());

/**
 * TMP-003: a versement on a contract. The amount starts at what is left on the oldest unpaid due
 * (the cashier may change it: the backend fills the oldest dues first); the droit de timbre on its
 * cash and what to take come from the backend; "Encaisser" records it on the contract's sale and
 * prints the `RV…` receipt (the drawer opens for cash).
 */
export function CollectDialog({ plan, onClose }: { plan: Plan; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const due = nextDue(plan);
  const [amount, setAmount] = useState(() => (due ? formatAmountInput(due.amount - due.paid, lang) : ""));
  const [method, setMethod] = useState<PaymentMethod>("cash");
  const [reference, setReference] = useState("");
  const [chequeBank, setChequeBank] = useState("");
  const [chequeDate, setChequeDate] = useState("");
  const [invalid, setInvalid] = useState(false);
  const hardware = useHardwareConfig();
  const noPrinter = hardware.data !== undefined && !hardware.data.receiptPrinterId;
  const collect = useCollectInstalment();
  const value = parseAmount(amount);
  const payment: PlanPayment | null =
    value !== null && value > 0
      ? {
          amount: value,
          method,
          reference: method === "cash" ? null : text(reference),
          chequeBank: method === "cheque" ? text(chequeBank) : null,
          chequeDate: method === "cheque" ? text(chequeDate) : null,
          note: null,
        }
      : null;
  // The duty and what to take, as M2's versement on the contract's sale computes them.
  const quote = useCreditQuote(
    payment && {
      customerId: plan.customerId,
      amount: payment.amount,
      method: payment.method,
      reference: payment.reference,
      chequeBank: payment.chequeBank,
      chequeDate: payment.chequeDate,
      allocateTo: [plan.creditEntryId],
      note: null,
    },
  );
  const figures = payment ? quote.data : undefined;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setInvalid(payment === null);
    if (payment === null) return;
    collect.mutate(
      { planId: plan.id, payment, printReceipt: !noPrinter },
      {
        onSuccess: (done) => {
          toast.success(t("instalments:collect.done", { number: done.collected.entry.receiptNumber ?? "" }));
          onClose();
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("instalments:plan.collect")}
      description={t("instalments:collect.for", { number: plan.number, customer: plan.customerName })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="instalment-collect" variant="primary" disabled={collect.isPending}>
            {t("instalments:collect.submit")}
          </Button>
        </>
      }
    >
      <form id="instalment-collect" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <Field
          id="collect-amount"
          label={t("instalments:collect.amount")}
          error={invalid ? t("validation:amountFormat") : undefined}
        >
          <NumericInput
            id="collect-amount"
            autoFocus
            value={amount}
            onValueChange={setAmount}
            className="text-2xl font-bold"
          />
        </Field>
        <Field id="collect-method" label={t("instalments:collect.method")}>
          <NativeSelect
            id="collect-method"
            value={method}
            onChange={(e) => setMethod(PAY_METHODS.find((m) => m === e.target.value) ?? "cash")}
          >
            {PAY_METHODS.map((m) => (
              <option key={m} value={m}>
                {t(`payments:tenders.${m}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        {method !== "cash" ? (
          <Field id="collect-reference" label={t("instalments:collect.reference")}>
            <GuardedInput
              id="collect-reference"
              dir="ltr"
              maxLength={40}
              value={reference}
              onValueChange={setReference}
            />
          </Field>
        ) : null}
        {method === "cheque" ? (
          <div className="grid gap-3 sm:grid-cols-2">
            <Field id="collect-bank" label={t("payments:cheque.bank")}>
              <GuardedInput
                id="collect-bank"
                maxLength={48}
                value={chequeBank}
                onValueChange={setChequeBank}
              />
            </Field>
            <Field id="collect-cheque-date" label={t("payments:cheque.date")}>
              <Input
                id="collect-cheque-date"
                type="date"
                dir="ltr"
                value={chequeDate}
                onChange={(e) => setChequeDate(e.target.value)}
              />
            </Field>
          </div>
        ) : null}
        {figures ? (
          <div role="status" className="flex flex-col gap-1 rounded-card bg-surface-2 p-4">
            {figures.stampCharged > 0 ? (
              <p>{t("instalments:collect.duty", { amount: formatDZD(figures.stampCharged, lang) })}</p>
            ) : null}
            <p className="text-xl font-bold">
              {t("instalments:collect.toReceive", { amount: formatDZD(figures.toReceive, lang) })}
            </p>
          </div>
        ) : null}
        {noPrinter ? <p className="text-sm text-ink-2">{t("payments:noPrinter")}</p> : null}
        {collect.error ? (
          <p role="alert" className="text-danger">
            {errorText(collect.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
