import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { SupplierSelect } from "@/features/purchases/purchase-lines";
import { useCan } from "@/features/stock/stock-parts";
import { today } from "@/lib/dates";
import { useCurrentSession } from "@/lib/ipc/cash";
import { useExpenseCategories, useRecordExpense } from "@/lib/ipc/expenses";
import type { NewExpense, PaymentMethod } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** How an expense can be paid (pos-core `expenses::record`); from the drawer it is cash. */
const METHODS: readonly PaymentMethod[] = ["cash", "card_cib", "edahabia", "baridimob", "cheque", "transfer"];

type Errors = Partial<Record<"categoryId" | "amount", string>>;

const text = (value: string) => (value.trim() === "" ? null : value.trim());

/**
 * CSH-002, FIN-001: "Nouvelle dépense". Paid with money from the drawer by default while this
 * terminal's register is open (the backend moves the drawer and the Z); otherwise, or unticked,
 * paid by any method on a day of the current year. Category, reference, supplier and detail are
 * kept with it; the amount is parsed to centimes, never computed.
 */
export function ExpenseDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const can = useCan();
  const record = useRecordExpense();
  const categories = useExpenseCategories(false).data ?? [];
  const drawerOpen = Boolean(useCurrentSession().data);
  const readsSuppliers = can("suppliers.manage") || can("purchases.view");
  const [categoryId, setCategoryId] = useState("");
  const [amount, setAmount] = useState("");
  const [method, setMethod] = useState<PaymentMethod>("cash");
  // `null`: the default, which is the drawer while it is open.
  const [tillChoice, setTillChoice] = useState<boolean | null>(null);
  const [day, setDay] = useState("");
  const [reference, setReference] = useState("");
  const [supplierId, setSupplierId] = useState("");
  const [note, setNote] = useState("");
  const [errors, setErrors] = useState<Errors>({});
  const fromTill = drawerOpen && (tillChoice ?? true);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const value = parseAmount(amount);
    const found: Errors = {};
    if (categoryId === "") found.categoryId = "validation:required";
    if (value === null || value <= 0) found.amount = "validation:amountFormat";
    setErrors(found);
    if (Object.keys(found).length > 0 || value === null) return;
    const input: NewExpense = {
      categoryId,
      amount: value,
      method: fromTill ? "cash" : method,
      fromTill,
      reference: text(reference),
      supplierId: supplierId === "" ? null : supplierId,
      businessDate: fromTill || day === "" ? null : day,
      note: text(note),
    };
    record.mutate(input, {
      onSuccess: () => {
        toast.success(t("cash:expenses.saved"));
        onClose();
      },
    });
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("cash:expenses.new")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="expense" variant="primary" disabled={record.isPending}>
            {t("cash:expense.submit")}
          </Button>
        </>
      }
    >
      <form id="expense" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <div className="grid gap-4 sm:grid-cols-2">
          <Field
            id="expense-category"
            label={t("cash:expense.category")}
            error={errors.categoryId ? t(errors.categoryId) : undefined}
          >
            <NativeSelect
              id="expense-category"
              value={categoryId}
              aria-invalid={Boolean(errors.categoryId)}
              onChange={(event) => setCategoryId(event.target.value)}
            >
              <option value="">{t("cash:expenses.chooseCategory")}</option>
              {categories.map((c) => (
                <option key={c.id} value={c.id}>
                  {localizedName(c, lang)}
                </option>
              ))}
            </NativeSelect>
          </Field>
          <Field
            id="expense-amount"
            label={t("cash:move.amount")}
            error={errors.amount ? t(errors.amount) : undefined}
          >
            <NumericInput
              id="expense-amount"
              aria-invalid={Boolean(errors.amount)}
              value={amount}
              onValueChange={setAmount}
            />
          </Field>
        </div>
        <div className="flex flex-col gap-1">
          <label className="flex min-h-11 items-center gap-3 font-semibold">
            <Checkbox
              checked={fromTill}
              disabled={!drawerOpen}
              onCheckedChange={(checked) => setTillChoice(checked === true)}
            />
            {t("cash:expenses.fromTill")}
          </label>
          <p className="text-sm text-ink-2">
            {drawerOpen ? t("cash:expenses.fromTillHint") : t("cash:expenses.tillClosed")}
          </p>
        </div>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field id="expense-method" label={t("cash:expenses.method")}>
            <NativeSelect
              id="expense-method"
              value={fromTill ? "cash" : method}
              disabled={fromTill}
              onChange={(event) => setMethod(event.target.value as PaymentMethod)}
            >
              {METHODS.map((m) => (
                <option key={m} value={m}>
                  {t(`payments:tenders.${m}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
          {fromTill ? null : (
            <Field id="expense-day" label={t("cash:expenses.day")} hint={t("cash:expenses.dayHint")}>
              <Input
                id="expense-day"
                type="date"
                dir="ltr"
                className="w-auto"
                max={today()}
                value={day === "" ? today() : day}
                onChange={(event) => setDay(event.target.value)}
              />
            </Field>
          )}
        </div>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field
            id="expense-reference"
            label={t("cash:expenses.reference")}
            optional={t("setup:shop.optional")}
          >
            <GuardedInput
              id="expense-reference"
              dir="ltr"
              maxLength={48}
              value={reference}
              onValueChange={setReference}
            />
          </Field>
          {readsSuppliers ? (
            <SupplierSelect
              id="expense-supplier"
              value={supplierId}
              onChange={setSupplierId}
              allLabel={t("cash:expenses.noSupplier")}
            />
          ) : null}
        </div>
        <Field id="expense-note" label={t("cash:expense.note")} optional={t("setup:shop.optional")}>
          <Input
            id="expense-note"
            dir="auto"
            maxLength={200}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </Field>
        {record.error ? (
          <p role="alert" className="text-danger">
            {errorText(record.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
