import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formatAmountInput, formatDZD } from "@/lib/format";
import { toAppError } from "@/lib/ipc/client";
import type { PaymentMethod, Stop, StopReport } from "@/lib/ipc/types";
import { useReportStop } from "@/lib/ipc/wholesale";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** What a stop's customer may pay the driver with (the versement of the close takes these). */
export const STOP_METHODS: readonly PaymentMethod[] = [
  "cash",
  "cheque",
  "transfer",
  "card_cib",
  "edahabia",
  "baridimob",
];

type Outcome = "delivered" | "failed";

const text = (value: string) => (value.trim() === "" ? null : value.trim());

/**
 * VRT-100: what happened at a stop of a round on the road — delivered or not, the amount collected
 * and how (cash; cheque with its bank, number and date; transfer with its reference; CIB,
 * Edahabia, BaridiMob). Never more than the customer owed on the document when the stop was
 * planned: the screen says so as soon as it is typed, and the backend refuses it too. It can be
 * reported again until the round is closed.
 */
export function StopReportDialog({
  roundId,
  stop,
  onClose,
}: {
  roundId: string;
  stop: Stop;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const report = useReportStop();
  const [outcome, setOutcome] = useState<Outcome>(stop.status === "failed" ? "failed" : "delivered");
  const [amount, setAmount] = useState(stop.collected > 0 ? formatAmountInput(stop.collected, lang) : "");
  const [method, setMethod] = useState<PaymentMethod>(stop.method ?? "cash");
  const [reference, setReference] = useState(stop.reference ?? "");
  const [chequeBank, setChequeBank] = useState(stop.chequeBank ?? "");
  const [chequeDate, setChequeDate] = useState(stop.chequeDate ?? "");
  const [note, setNote] = useState(stop.note ?? "");
  const [invalid, setInvalid] = useState(false);
  const delivered = outcome === "delivered";
  const value = amount.trim() === "" ? 0 : parseAmount(amount);
  const above = delivered && value !== null && value > stop.amountDue;
  const collecting = delivered && value !== null && value > 0;
  const due = t("wholesale:rounds.due", { amount: formatDZD(stop.amountDue, lang) });
  const refused = toAppError(report.error);
  const fieldError = (field: string) =>
    report.error && refused.field === field ? errorText(report.error) : undefined;
  const placed = ["collected", "method", "reference", "chequeBank", "chequeDate"].includes(
    refused.field ?? "",
  );
  const customer = stop.customerName ?? stop.documentNumber ?? "";

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (value === null) {
      setInvalid(true);
      return;
    }
    if (above) return;
    const collected = delivered ? value : 0;
    const paid = collected > 0;
    const sent: StopReport = {
      seq: stop.seq,
      status: outcome,
      collected,
      method: paid ? method : null,
      reference: paid && method !== "cash" ? text(reference) : null,
      chequeBank: paid && method === "cheque" ? text(chequeBank) : null,
      chequeDate: paid && method === "cheque" ? text(chequeDate) : null,
      note: text(note),
    };
    report.mutate(
      { id: roundId, report: sent },
      {
        onSuccess: () => {
          toast.success(t("wholesale:rounds.stopSaved", { seq: stop.seq }));
          onClose();
        },
      },
    );
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("wholesale:rounds.stop", { seq: stop.seq, customer })}
      description={[stop.documentNumber, stop.address, stop.phone].filter(Boolean).join(" · ")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="stop-report" variant="primary" disabled={report.isPending || above}>
            {t("wholesale:rounds.saveStop")}
          </Button>
        </>
      }
    >
      <form id="stop-report" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <RadioCards
          legend={t("wholesale:rounds.state")}
          name="stop-outcome"
          value={outcome}
          onChange={setOutcome}
          options={[
            { value: "delivered", label: t("wholesale:rounds.delivered") },
            { value: "failed", label: t("wholesale:rounds.failed") },
          ]}
        />
        {delivered ? (
          <>
            <Field
              id="stop-collected"
              label={t("wholesale:rounds.collected")}
              hint={above ? undefined : due}
              error={above ? due : invalid ? t("validation:amountFormat") : fieldError("collected")}
            >
              <NumericInput
                id="stop-collected"
                autoFocus
                aria-invalid={above || invalid}
                className="text-2xl font-bold"
                value={amount}
                onValueChange={(v) => {
                  setAmount(v);
                  setInvalid(false);
                }}
              />
            </Field>
            {collecting ? (
              <>
                <RadioCards
                  legend={t("wholesale:rounds.method")}
                  name="stop-method"
                  value={method}
                  onChange={setMethod}
                  options={STOP_METHODS.map((m) => ({ value: m, label: t(`payments:tenders.${m}`) }))}
                />
                {fieldError("method") ? (
                  <p role="alert" className="text-sm text-danger">
                    {fieldError("method")}
                  </p>
                ) : null}
                {method !== "cash" ? (
                  <Field
                    id="stop-reference"
                    label={t(`payments:reference.${method}`, { method: t(`payments:tenders.${method}`) })}
                    error={fieldError("reference")}
                  >
                    <GuardedInput
                      id="stop-reference"
                      dir="ltr"
                      maxLength={60}
                      value={reference}
                      onValueChange={setReference}
                    />
                  </Field>
                ) : null}
                {method === "cheque" ? (
                  <div className="grid gap-3 sm:grid-cols-2">
                    <Field id="stop-bank" label={t("payments:cheque.bank")} error={fieldError("chequeBank")}>
                      <GuardedInput
                        id="stop-bank"
                        maxLength={60}
                        value={chequeBank}
                        onValueChange={setChequeBank}
                      />
                    </Field>
                    <Field
                      id="stop-cheque-date"
                      label={t("payments:cheque.date")}
                      error={fieldError("chequeDate")}
                    >
                      <Input
                        id="stop-cheque-date"
                        type="date"
                        dir="ltr"
                        value={chequeDate}
                        onChange={(e) => setChequeDate(e.target.value)}
                      />
                    </Field>
                  </div>
                ) : null}
              </>
            ) : null}
          </>
        ) : null}
        <Field id="stop-note" label={t("wholesale:rounds.note")} optional={t("wholesale:rounds.optional")}>
          <Input
            id="stop-note"
            dir="auto"
            maxLength={200}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </Field>
        {report.error && !placed ? (
          <p role="alert" className="text-danger">
            {errorText(report.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
