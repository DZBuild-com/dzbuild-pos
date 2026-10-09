import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import type { PaymentInput, Settlement } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";

/** The tenders an invoice takes at the desk, in this order (PAY-001); `credit` is the customer's
 * account ("À terme"). */
const METHODS = ["cash", "card_cib", "edahabia", "baridimob", "cheque", "transfer", "credit"] as const;
type Method = (typeof METHODS)[number];

interface Typed {
  amount: string;
  reference: string;
  bank: string;
  date: string;
}

const EMPTY: Typed = { amount: "", reference: "", bank: "", date: "" };

function blank(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

/** The typed amounts as integer centimes, zero and unreadable ones left out, with a cheque's bank,
 * date and number and a transfer's reference. */
function toTenders(typed: Record<Method, Typed>, withCredit: boolean): PaymentInput[] {
  return METHODS.flatMap((method) => {
    if (method === "credit" && !withCredit) return [];
    const t = typed[method];
    const amount = parseAmount(t.amount);
    if (amount === null || amount <= 0) return [];
    const detailed = method === "cheque" || method === "transfer";
    return [
      {
        method,
        amount,
        reference: detailed ? blank(t.reference) : null,
        chequeBank: method === "cheque" ? blank(t.bank) : null,
        chequeDate: method === "cheque" ? blank(t.date) : null,
      },
    ];
  });
}

/**
 * How an invoice is settled at the desk: one amount per tender (cash, CIB card, Edahabia,
 * BaridiMob, cheque with its bank, date and number, transfer with its reference, and "À terme" on
 * the customer's account when the invoice has one). It answers the tenders to its parent, which
 * asks the backend for the duty, the net to pay and what is left: nothing is computed here.
 */
export function TendersEditor({
  withCredit,
  onChange,
  disabled = false,
}: {
  withCredit: boolean;
  onChange: (tenders: PaymentInput[]) => void;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const id = useId();
  const [typed, setTyped] = useState<Record<Method, Typed>>(
    () => Object.fromEntries(METHODS.map((m) => [m, EMPTY])) as Record<Method, Typed>,
  );
  const update = (method: Method, patch: Partial<Typed>) => {
    const next = { ...typed, [method]: { ...typed[method], ...patch } };
    setTyped(next);
    onChange(toTenders(next, withCredit));
  };
  const label = (method: Method) =>
    method === "credit" ? t("billing:validate.onAccount") : t(`payments:tenders.${method}`);
  return (
    <fieldset className="flex flex-col gap-3" disabled={disabled}>
      <legend className="mb-2 font-bold">{t("billing:validate.tenders")}</legend>
      <div className="grid gap-3 sm:grid-cols-2">
        {METHODS.filter((m) => m !== "credit" || withCredit).map((method) => {
          const field = `${id}-${method}`;
          const value = typed[method];
          const shown = value.amount.trim() !== "";
          return (
            <div key={method} className="flex flex-col gap-2">
              <Field id={field} label={label(method)}>
                <NumericInput
                  id={field}
                  value={value.amount}
                  onValueChange={(amount) => update(method, { amount })}
                />
              </Field>
              {method === "cheque" && shown ? (
                <>
                  <Field id={`${field}-bank`} label={t("billing:validate.chequeBank")}>
                    <GuardedInput
                      id={`${field}-bank`}
                      maxLength={60}
                      value={value.bank}
                      onValueChange={(bank) => update(method, { bank })}
                    />
                  </Field>
                  <Field id={`${field}-date`} label={t("billing:validate.chequeDate")}>
                    <Input
                      id={`${field}-date`}
                      type="date"
                      dir="ltr"
                      value={value.date}
                      onChange={(event) => update(method, { date: event.target.value })}
                    />
                  </Field>
                  <Field id={`${field}-ref`} label={t("payments:reference.cheque")}>
                    <GuardedInput
                      id={`${field}-ref`}
                      dir="ltr"
                      maxLength={60}
                      value={value.reference}
                      onValueChange={(reference) => update(method, { reference })}
                    />
                  </Field>
                </>
              ) : null}
              {method === "transfer" && shown ? (
                <Field id={`${field}-ref`} label={t("billing:validate.reference")}>
                  <GuardedInput
                    id={`${field}-ref`}
                    dir="ltr"
                    maxLength={60}
                    value={value.reference}
                    onValueChange={(reference) => update(method, { reference })}
                  />
                </Field>
              ) : null}
            </div>
          );
        })}
      </div>
    </fieldset>
  );
}

/** The backend's settlement of the typed tenders: total, stamp duty on the cash part, net to
 * pay, what is left and the change. */
export function SettlementFigures({ settlement }: { settlement: Settlement }) {
  const { t } = useTranslation();
  const rows: { label: string; value: number; strong?: boolean; tone?: string }[] = [
    { label: t("billing:editor.totals.ttc"), value: settlement.totalTtc },
    { label: t("billing:validate.duty"), value: settlement.stampDuty },
    { label: t("billing:validate.netToPay"), value: settlement.totalDue, strong: true },
    {
      label: t("billing:validate.missing"),
      value: settlement.missing,
      tone: settlement.missing > 0 ? "text-danger" : undefined,
    },
    { label: t("billing:validate.change"), value: settlement.change },
  ];
  return (
    <dl className="grid grid-cols-[1fr_auto] gap-x-6 gap-y-1">
      {rows.map(({ label, value, strong, tone }) => (
        <div key={label} className={`contents ${strong ? "text-lg font-bold" : ""} ${tone ?? ""}`}>
          <dt>{label}</dt>
          <dd className="text-end">
            <Amount value={value} />
          </dd>
        </div>
      ))}
    </dl>
  );
}
