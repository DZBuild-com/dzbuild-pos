import { useMutation } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { formatAmountInput, formatDZD } from "@/lib/format";
import { useDepositQuote, useRefundDeposit, useTakeDeposit } from "@/lib/ipc/services";
import type { DepositInput, OwnerType, PaymentMethod } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** How a deposit is paid or given back: cash (the drawer and its stamp duty) or a traced method
 * with its reference. */
const METHODS: readonly PaymentMethod[] = ["cash", "card_cib", "edahabia", "baridimob", "transfer"];

function MethodField({
  id,
  method,
  onMethod,
  reference,
  onReference,
}: {
  id: string;
  method: PaymentMethod;
  onMethod: (method: PaymentMethod) => void;
  reference: string;
  onReference: (reference: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <>
      <Field id={`${id}-method`} label={t("services:deposit.method")}>
        <NativeSelect
          id={`${id}-method`}
          value={method}
          onChange={(event) => onMethod(METHODS.find((m) => m === event.target.value) ?? "cash")}
        >
          {METHODS.map((m) => (
            <option key={m} value={m}>
              {t(`payments:tenders.${m}`)}
            </option>
          ))}
        </NativeSelect>
      </Field>
      {method === "cash" ? null : (
        <Field id={`${id}-reference`} label={t("services:deposit.reference")}>
          <GuardedInput
            id={`${id}-reference`}
            dir="ltr"
            maxLength={40}
            value={reference}
            onValueChange={onReference}
          />
        </Field>
      )}
    </>
  );
}

/**
 * CRD-007: arrhes on a repair or service ticket. The stamp duty on its cash and what the customer
 * hands over come from `deposits_quote`; "Encaisser les arrhes" records it (on the main PC from a
 * till) and prints the `AC…` receipt. After an intake it is offered, with "Sans arrhes". A
 * customer order's further deposit goes through its own write (`onTake`: `orders_pay_in`).
 */
export function DepositDialog({
  ownerType,
  ownerId,
  heading,
  offered = false,
  onTake,
  onClose,
}: {
  ownerType: OwnerType;
  ownerId: string;
  /** The ticket's number and customer, shown under the title. */
  heading: string;
  /** Offered right after the intake: the way out says "Sans arrhes". */
  offered?: boolean;
  /** Records the deposit instead of `deposits_take`. */
  onTake?: (input: DepositInput) => Promise<unknown>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [amount, setAmount] = useState("");
  const [method, setMethod] = useState<PaymentMethod>("cash");
  const [reference, setReference] = useState("");
  const [invalid, setInvalid] = useState(false);
  const value = parseAmount(amount);
  const input: DepositInput | null =
    value !== null && value > 0
      ? {
          ownerType,
          ownerId,
          amount: value,
          method,
          reference: method === "cash" || reference.trim() === "" ? null : reference.trim(),
          chequeBank: null,
          chequeDate: null,
          note: null,
        }
      : null;
  const quote = useDepositQuote(input);
  const takeDeposit = useTakeDeposit();
  const take = useMutation({
    mutationFn: (deposit: DepositInput) => (onTake ? onTake(deposit) : takeDeposit.mutateAsync(deposit)),
  });
  const submit = (event: FormEvent) => {
    event.preventDefault();
    setInvalid(input === null);
    if (input === null) return;
    take.mutate(input, {
      onSuccess: () => {
        toast.success(t("services:deposit.taken"));
        onClose();
      },
    });
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("services:deposit.title")}
      description={heading}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {offered ? t("services:deposit.skip") : t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="deposit-take" variant="primary" disabled={take.isPending}>
            {t("services:deposit.take")}
          </Button>
        </>
      }
    >
      <form id="deposit-take" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <Field
          id="deposit-amount"
          label={t("services:deposit.amount")}
          error={invalid ? t("validation:amountFormat") : undefined}
        >
          <NumericInput id="deposit-amount" autoFocus value={amount} onValueChange={setAmount} />
        </Field>
        <MethodField
          id="deposit"
          method={method}
          onMethod={setMethod}
          reference={reference}
          onReference={setReference}
        />
        {input && quote.data ? (
          <div role="status" className="flex flex-col gap-1 rounded-control bg-surface-2 p-3">
            {quote.data.stampDuty > 0 ? (
              <p>{t("services:deposit.duty", { amount: formatDZD(quote.data.stampDuty, lang) })}</p>
            ) : null}
            <p className="text-lg font-bold">
              {t("services:deposit.toReceive", { amount: formatDZD(quote.data.toReceive, lang) })}
            </p>
          </div>
        ) : null}
        {quote.error ? (
          <p role="alert" className="text-danger">
            {errorText(quote.error)}
          </p>
        ) : null}
        {take.error ? (
          <p role="alert" className="text-danger">
            {errorText(take.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}

/** Gives back what is held on a ticket, never more (`sales.return`); cash opens the drawer. */
export function RefundDialog({
  ownerType,
  ownerId,
  held,
  heading,
  onClose,
}: {
  ownerType: OwnerType;
  ownerId: string;
  held: number;
  heading: string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [amount, setAmount] = useState(() => formatAmountInput(held, lang));
  const [method, setMethod] = useState<PaymentMethod>("cash");
  const [reference, setReference] = useState("");
  const [invalid, setInvalid] = useState(false);
  const refund = useRefundDeposit();
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const value = parseAmount(amount);
    const bad = value === null || value <= 0;
    setInvalid(bad);
    if (bad) return;
    refund.mutate(
      {
        ownerType,
        ownerId,
        amount: value,
        method,
        reference: method === "cash" || reference.trim() === "" ? null : reference.trim(),
      },
      {
        onSuccess: () => {
          toast.success(t("services:deposit.refunded"));
          onClose();
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("services:deposit.refund")}
      description={heading}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="deposit-refund" variant="primary" disabled={refund.isPending}>
            {t("services:deposit.refund")}
          </Button>
        </>
      }
    >
      <form id="deposit-refund" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <p className="font-semibold">{t("services:deposit.held", { amount: formatDZD(held, lang) })}</p>
        <Field
          id="refund-amount"
          label={t("services:deposit.amount")}
          error={invalid ? t("validation:amountFormat") : undefined}
        >
          <NumericInput id="refund-amount" autoFocus value={amount} onValueChange={setAmount} />
        </Field>
        <MethodField
          id="refund"
          method={method}
          onMethod={setMethod}
          reference={reference}
          onReference={setReference}
        />
        {refund.error ? (
          <p role="alert" className="text-danger">
            {errorText(refund.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
