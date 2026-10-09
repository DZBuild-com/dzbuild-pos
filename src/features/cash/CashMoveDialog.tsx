import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useCashMove } from "@/lib/ipc/cash";
import type { ManualKind } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";

/** CSH-001: money in or out of the drawer, or a drop to the safe, always with a reason. */
export function CashMoveDialog({ kind, onClose }: { kind: ManualKind; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const move = useCashMove();
  const [amount, setAmount] = useState("");
  const [note, setNote] = useState("");
  const [errors, setErrors] = useState<{ amount?: string; note?: string }>({});
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const value = parseAmount(amount);
    const found = {
      amount: value === null || value <= 0 ? t("validation:amountFormat") : undefined,
      note: note.trim() === "" ? t("validation:required") : undefined,
    };
    setErrors(found);
    if (value === null || value <= 0 || found.note) return;
    move.mutate({ kind, amount: value, note: note.trim() }, { onSuccess: onClose });
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t(`cash:move.${kind}.title`)}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="cash-move" variant="primary" disabled={move.isPending}>
            {t(`cash:move.${kind}.submit`)}
          </Button>
        </>
      }
    >
      <form id="cash-move" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <Field id="move-amount" label={t("cash:move.amount")} error={errors.amount}>
          <NumericInput id="move-amount" autoFocus value={amount} onValueChange={setAmount} />
        </Field>
        <Field id="move-note" label={t("cash:move.note")} error={errors.note}>
          <Input id="move-note" value={note} maxLength={200} onChange={(e) => setNote(e.target.value)} />
        </Field>
        {move.error ? (
          <p role="alert" className="text-danger">
            {errorText(move.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
