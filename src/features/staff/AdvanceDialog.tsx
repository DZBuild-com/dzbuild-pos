import { type FormEvent, type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { MethodPicker } from "@/features/customers/VersementDialog";
import { today } from "@/lib/dates";
import { formatAmountInput } from "@/lib/format";
import { useAdvance, useDeduct, useRepay } from "@/lib/ipc/staff";
import type { PaymentMethod, StaffProfile } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const text = (value: string) => (value.trim() === "" ? null : value.trim());

/** A typed amount in DA → centimes; `null` unless above zero (the backend checks it again). */
function positive(value: string): number | null {
  const centimes = parseAmount(value);
  return centimes !== null && centimes > 0 ? centimes : null;
}

/** The frame of the three ledger dialogs: the employee in the description, the amount, the
 * dialog's own fields, the note and the backend's refusal. */
function LedgerDialog({
  id,
  title,
  employee,
  hint,
  amount,
  onAmount,
  invalid,
  note,
  onNote,
  error,
  submit,
  pending,
  onSubmit,
  onClose,
  children,
}: {
  id: string;
  title: string;
  employee: StaffProfile;
  hint: string | null;
  amount: string;
  onAmount: (value: string) => void;
  invalid: boolean;
  note: string;
  onNote: (value: string) => void;
  error: unknown;
  submit: string;
  pending: boolean;
  onSubmit: () => void;
  onClose: () => void;
  children?: ReactNode;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={title}
      description={employee.displayName}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form={id} variant="primary" disabled={pending}>
            {submit}
          </Button>
        </>
      }
    >
      <form
        id={id}
        noValidate
        onSubmit={(event: FormEvent) => {
          event.preventDefault();
          onSubmit();
        }}
        className="flex flex-col gap-4"
      >
        {hint ? <p className="text-ink-2">{hint}</p> : null}
        <Field
          id={`${id}-amount`}
          label={t("staff:advance.amount")}
          error={invalid ? t("validation:amountFormat") : undefined}
        >
          <NumericInput
            id={`${id}-amount`}
            autoFocus
            aria-invalid={invalid}
            value={amount}
            onValueChange={onAmount}
            className="text-2xl font-bold"
          />
        </Field>
        {children}
        <Field id={`${id}-note`} label={t("staff:advance.note")} optional={t("customers:form.optional")}>
          <Input
            id={`${id}-note`}
            dir="auto"
            maxLength={200}
            value={note}
            onChange={(event) => onNote(event.target.value)}
          />
        </Field>
        {error ? (
          <p role="alert" className="text-danger">
            {errorText(error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}

/** FIN/EMP: money given ahead of the salary — a wages expense, out of this PC's open drawer when
 * "Payée depuis la caisse" (cash only), else paid another way. */
export function AdvanceDialog({ employee, onClose }: { employee: StaffProfile; onClose: () => void }) {
  const { t } = useTranslation();
  const advance = useAdvance();
  const [amount, setAmount] = useState("");
  const [fromTill, setFromTill] = useState(false);
  const [method, setMethod] = useState<PaymentMethod>("cash");
  const [note, setNote] = useState("");
  const [invalid, setInvalid] = useState(false);
  const submit = () => {
    const value = positive(amount);
    if (value === null) {
      setInvalid(true);
      return;
    }
    advance.mutate(
      {
        userId: employee.userId,
        amount: value,
        method: fromTill ? "cash" : method,
        fromTill,
        note: text(note),
      },
      {
        onSuccess: () => {
          toast.success(t("staff:advance.given"));
          onClose();
        },
      },
    );
  };
  return (
    <LedgerDialog
      id="staff-advance"
      title={t("staff:advance.give")}
      employee={employee}
      hint={null}
      amount={amount}
      onAmount={(value) => {
        setAmount(value);
        setInvalid(false);
      }}
      invalid={invalid}
      note={note}
      onNote={setNote}
      error={advance.error}
      submit={t("staff:advance.submit")}
      pending={advance.isPending}
      onSubmit={submit}
      onClose={onClose}
    >
      <div className="flex flex-col gap-1">
        <label className="flex min-h-11 items-center gap-3 font-semibold">
          <Checkbox checked={fromTill} onCheckedChange={(value) => setFromTill(value === true)} />
          <span>{t("staff:advance.fromTill")}</span>
        </label>
        <p className="text-sm text-ink-2">{t("staff:advance.fromTillHint")}</p>
      </div>
      {fromTill ? null : (
        <div className="flex flex-col gap-2">
          <p className="font-semibold">{t("staff:advance.method")}</p>
          <MethodPicker value={method} onChange={setMethod} />
        </div>
      )}
    </LedgerDialog>
  );
}

/** The employee pays back part of what they owe; in cash it goes into this PC's open drawer. */
export function RepayDialog({ employee, onClose }: { employee: StaffProfile; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const repay = useRepay();
  const [amount, setAmount] = useState(() => formatAmountInput(Math.max(employee.owed, 0), lang));
  const [method, setMethod] = useState<PaymentMethod>("cash");
  const [note, setNote] = useState("");
  const [invalid, setInvalid] = useState(false);
  const submit = () => {
    const value = positive(amount);
    if (value === null) {
      setInvalid(true);
      return;
    }
    repay.mutate(
      { userId: employee.userId, amount: value, method, note: text(note) },
      {
        onSuccess: () => {
          toast.success(t("staff:advance.repaid"));
          onClose();
        },
      },
    );
  };
  return (
    <LedgerDialog
      id="staff-repay"
      title={t("staff:advance.repay")}
      employee={employee}
      hint={t("staff:advance.repayHint")}
      amount={amount}
      onAmount={(value) => {
        setAmount(value);
        setInvalid(false);
      }}
      invalid={invalid}
      note={note}
      onNote={setNote}
      error={repay.error}
      submit={t("staff:advance.repaySubmit")}
      pending={repay.isPending}
      onSubmit={submit}
      onClose={onClose}
    >
      <div className="flex flex-col gap-2">
        <p className="font-semibold">{t("staff:advance.method")}</p>
        <MethodPicker value={method} onChange={setMethod} />
      </div>
    </LedgerDialog>
  );
}

/** Part of the advances kept off a month's salary: no money moves. */
export function DeductDialog({ employee, onClose }: { employee: StaffProfile; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const deduct = useDeduct();
  const [amount, setAmount] = useState(() => formatAmountInput(Math.max(employee.owed, 0), lang));
  const [month, setMonth] = useState(() => today().slice(0, 7));
  const [note, setNote] = useState("");
  const [invalid, setInvalid] = useState(false);
  const monthOk = /^\d{4}-(0[1-9]|1[0-2])$/.test(month);
  const [tried, setTried] = useState(false);
  const submit = () => {
    setTried(true);
    const value = positive(amount);
    if (value === null) setInvalid(true);
    if (value === null || !monthOk) return;
    deduct.mutate(
      { userId: employee.userId, amount: value, month, note: text(note) },
      {
        onSuccess: () => {
          toast.success(t("staff:advance.deducted"));
          onClose();
        },
      },
    );
  };
  return (
    <LedgerDialog
      id="staff-deduct"
      title={t("staff:advance.deduct")}
      employee={employee}
      hint={t("staff:advance.deductHint")}
      amount={amount}
      onAmount={(value) => {
        setAmount(value);
        setInvalid(false);
      }}
      invalid={invalid}
      note={note}
      onNote={setNote}
      error={deduct.error}
      submit={t("staff:advance.deductSubmit")}
      pending={deduct.isPending}
      onSubmit={submit}
      onClose={onClose}
    >
      <Field
        id="staff-deduct-month"
        label={t("staff:advance.month")}
        error={tried && !monthOk ? t("errors:month_invalid") : undefined}
      >
        <Input
          id="staff-deduct-month"
          type="month"
          dir="ltr"
          className="max-w-56"
          value={month}
          onChange={(event) => setMonth(event.target.value)}
        />
      </Field>
    </LedgerDialog>
  );
}
