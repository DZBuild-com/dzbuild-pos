import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { useApproval } from "@/components/manager-approval";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { isApprovalCancelled } from "@/lib/approval";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useCollect, useCreditQuote, useOpenItems } from "@/lib/ipc/credit";
import { useHardwareConfig } from "@/lib/ipc/hardware";
import type { CollectInput, Customer, LedgerEntry, PaymentMethod } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** How a versement can be paid: never on the account itself (credit, store credit, change). */
const METHODS: readonly PaymentMethod[] = ["cash", "card_cib", "edahabia", "baridimob", "cheque", "transfer"];

export interface Details {
  reference: string;
  chequeBank: string;
  chequeDate: string;
}

const text = (value: string) => (value.trim() === "" ? null : value.trim());

/** The payment tiles of the till's payment dialog (spec §10.5), one method at a time; a gift
 * voucher sold (M8) is paid the same ways. */
export function MethodPicker({
  value,
  onChange,
}: {
  value: PaymentMethod;
  onChange: (m: PaymentMethod) => void;
}) {
  const { t } = useTranslation();
  return (
    <div role="radiogroup" aria-label={t("customers:versement.method")} className="grid grid-cols-3 gap-2">
      {METHODS.map((method) => (
        <button
          key={method}
          type="button"
          role="radio"
          aria-checked={value === method}
          onClick={() => onChange(method)}
          className={cn(
            "min-h-14 rounded-control border px-2 font-semibold",
            value === method ? "border-brand bg-brand-soft" : "border-line bg-surface-2",
          )}
        >
          {t(`payments:tenders.${method}`)}
        </button>
      ))}
    </div>
  );
}

/** The number a card, BaridiMob, cheque or transfer versement carries on its receipt (PAY-008);
 * a cheque also needs its bank and date, or the backend charges the droit de timbre. `idPrefix`
 * names its fields (`versement-reference`…). */
export function MethodDetails({
  method,
  value,
  onChange,
  idPrefix = "versement",
}: {
  method: PaymentMethod;
  value: Details;
  onChange: (value: Details) => void;
  idPrefix?: string;
}) {
  const { t } = useTranslation();
  if (method === "cash") return null;
  return (
    <div className="grid gap-3 sm:grid-cols-3">
      <Field
        id={`${idPrefix}-reference`}
        label={t(`payments:reference.${method}`, { method: t(`payments:tenders.${method}`) })}
      >
        <GuardedInput
          id={`${idPrefix}-reference`}
          dir="ltr"
          maxLength={40}
          value={value.reference}
          onValueChange={(reference) => onChange({ ...value, reference })}
        />
      </Field>
      {method === "cheque" ? (
        <>
          <Field id={`${idPrefix}-bank`} label={t("payments:cheque.bank")}>
            <GuardedInput
              id={`${idPrefix}-bank`}
              maxLength={48}
              value={value.chequeBank}
              onValueChange={(chequeBank) => onChange({ ...value, chequeBank })}
            />
          </Field>
          <Field id={`${idPrefix}-cheque-date`} label={t("payments:cheque.date")}>
            <Input
              id={`${idPrefix}-cheque-date`}
              type="date"
              dir="ltr"
              value={value.chequeDate}
              onChange={(event) => onChange({ ...value, chequeDate: event.target.value })}
            />
          </Field>
        </>
      ) : null}
    </div>
  );
}

/** A debt the versement may settle: its document (or its kind) and what is still owed on it. */
function OpenItem({
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
 * CRD-004, TAX-007: a versement on the kridi account. The droit de timbre on the cash, what to
 * take and the balance after all come from `credit_quote` as the cashier types (the UI never
 * computes them); the debts are settled oldest first unless some are chosen; the numbered `RV…`
 * receipt prints through the queue (and the drawer opens for cash).
 */
export function VersementDialog({
  customer,
  open,
  onClose,
}: {
  customer: Customer;
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const [amount, setAmount] = useState("");
  const [method, setMethod] = useState<PaymentMethod>("cash");
  const [details, setDetails] = useState<Details>({ reference: "", chequeBank: "", chequeDate: "" });
  const [chooseDebts, setChooseDebts] = useState(false);
  const [chosen, setChosen] = useState<string[]>([]);
  const [note, setNote] = useState("");
  // Printed unless the cashier unticks it, or the shop has no receipt printer at all.
  const hardware = useHardwareConfig();
  const [printChoice, setPrintChoice] = useState<boolean | null>(null);
  const noPrinter = hardware.data !== undefined && !hardware.data.receiptPrinterId;
  const print = printChoice ?? !noPrinter;
  const openItems = useOpenItems(open ? customer.id : null);
  const debts = openItems.data ?? [];
  const collect = useCollect();
  const approval = useApproval();
  const [error, setError] = useState<unknown>(null);

  const value = parseAmount(amount);
  const input: CollectInput | null =
    value !== null && value > 0
      ? {
          customerId: customer.id,
          amount: value,
          method,
          reference: method === "cash" ? null : text(details.reference),
          chequeBank: method === "cheque" ? text(details.chequeBank) : null,
          chequeDate: method === "cheque" ? text(details.chequeDate) : null,
          allocateTo: chooseDebts ? debts.filter((d) => chosen.includes(d.id)).map((d) => d.id) : [],
          note: text(note),
        }
      : null;
  // The note changes nothing in the figures: typing it asks the backend nothing.
  const quote = useCreditQuote(input && { ...input, note: null });
  const figures = input ? quote.data : undefined;
  const ready = input !== null && figures !== undefined && !quote.isFetching && !quote.error;

  const submit = async () => {
    if (!input || !ready) return;
    setError(null);
    try {
      const done = await approval.run(() => collect.mutateAsync({ input, printReceipt: print }));
      toast.success(t("customers:versement.done", { number: done.collected.entry.receiptNumber ?? "" }));
      onClose();
    } catch (e) {
      if (!isApprovalCancelled(e)) setError(e);
    }
  };

  const toggle = (id: string) =>
    setChosen((ids) => (ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id]));
  const problem = error ?? (input ? quote.error : null);
  return (
    <>
      <Dialog
        open={open && !approval.pending}
        size="lg"
        onOpenChange={(o) => (o ? undefined : onClose())}
        title={t("customers:versement.title", { name: customer.name })}
        footer={
          <>
            <Button variant="outline" onClick={onClose}>
              {t("common:actions.cancel")}
            </Button>
            <Button variant="primary" disabled={!ready || collect.isPending} onClick={() => void submit()}>
              {t("customers:versement.collect")}
            </Button>
          </>
        }
      >
        <form
          noValidate
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          <Field id="versement-amount" label={t("customers:versement.amount")}>
            <NumericInput
              id="versement-amount"
              autoFocus
              value={amount}
              onValueChange={(v) => {
                setAmount(v);
                setError(null);
              }}
              className="text-2xl font-bold"
            />
          </Field>
          <MethodPicker
            value={method}
            onChange={(m) => {
              setMethod(m);
              setError(null);
            }}
          />
          <MethodDetails method={method} value={details} onChange={setDetails} />
          {figures ? (
            <dl
              aria-live="polite"
              className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-2 rounded-card bg-surface-2 p-4"
            >
              <dt>{t("customers:versement.balanceBefore")}</dt>
              <dd className="text-end">
                <Amount value={figures.balanceBefore} />
              </dd>
              {figures.stampCharged > 0 ? (
                <>
                  <dt>{t("customers:versement.stampDuty")}</dt>
                  <dd className="text-end">
                    <Amount value={figures.stampCharged} />
                  </dd>
                </>
              ) : null}
              {figures.stampDuty > figures.stampCharged ? (
                <>
                  <dt className="text-ink-2">{t("customers:versement.stampAbsorbed")}</dt>
                  <dd className="text-end text-ink-2">
                    <Amount value={figures.stampDuty} />
                  </dd>
                </>
              ) : null}
              <dt className="text-xl font-bold">
                {t(method === "cash" ? "customers:versement.cashToTake" : "customers:versement.toReceive")}
              </dt>
              <dd className="text-end text-xl font-bold">
                <Amount value={figures.toReceive} />
              </dd>
              <dt>{t("customers:versement.balanceAfter")}</dt>
              <dd className="flex flex-wrap items-center justify-end gap-2">
                {figures.balanceAfter < 0 ? (
                  <Badge tone="info">{t("customers:versement.advanceAfter")}</Badge>
                ) : null}
                <Amount value={figures.balanceAfter} className="font-semibold" />
              </dd>
            </dl>
          ) : null}
          {debts.length > 1 ? (
            <fieldset className="flex flex-col gap-2">
              <legend className="sr-only">{t("customers:detail.openItems")}</legend>
              <label className="flex min-h-11 items-center gap-3">
                <input
                  type="radio"
                  name="versement-allocation"
                  className="size-5 accent-brand"
                  checked={!chooseDebts}
                  onChange={() => setChooseDebts(false)}
                />
                {t("customers:versement.allocateOldest")}
              </label>
              <label className="flex min-h-11 items-center gap-3">
                <input
                  type="radio"
                  name="versement-allocation"
                  className="size-5 accent-brand"
                  checked={chooseDebts}
                  onChange={() => setChooseDebts(true)}
                />
                {t("customers:versement.allocateChosen")}
              </label>
              {chooseDebts ? (
                <div className="flex flex-col gap-1 rounded-card border border-line p-2">
                  <p className="px-2 text-sm text-ink-2">{t("customers:versement.chosenHint")}</p>
                  {debts.map((item) => (
                    <OpenItem
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
          <Field
            id="versement-note"
            label={t("customers:versement.note")}
            optional={t("customers:form.optional")}
          >
            <Input
              id="versement-note"
              dir="auto"
              maxLength={200}
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </Field>
          <label className="flex items-center gap-3 font-semibold">
            <Checkbox
              checked={print}
              disabled={noPrinter}
              onCheckedChange={(v) => setPrintChoice(v === true)}
            />
            {t("customers:versement.print")}
          </label>
          {noPrinter ? <p className="text-sm text-ink-2">{t("payments:noPrinter")}</p> : null}
          {problem ? (
            <p role="alert" className="text-danger">
              {errorText(problem)}
            </p>
          ) : null}
        </form>
      </Dialog>
      {approval.dialog}
    </>
  );
}
