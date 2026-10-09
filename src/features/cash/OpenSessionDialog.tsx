import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { formatAmountInput } from "@/lib/format";
import { useOpenSession } from "@/lib/ipc/cash";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { CountSheet } from "./CountSheet";
import { type Counts, countedTotal, emptyCounts } from "./denominations";

/** SAL-001: open the till with the counted float (fond de caisse). */
export function OpenSessionDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const openSession = useOpenSession();
  const [amount, setAmount] = useState("");
  const [counting, setCounting] = useState(false);
  const [counts, setCounts] = useState<Counts>(emptyCounts);
  const [invalid, setInvalid] = useState(false);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const float = amount.trim() === "" ? 0 : parseAmount(amount);
    if (float === null) {
      setInvalid(true);
      return;
    }
    openSession.mutate(float, { onSuccess: () => onOpenChange(false) });
  };
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      size={counting ? "lg" : "md"}
      title={t("cash:open.title")}
      description={t("cash:open.hint")}
      footer={
        <>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="open-session" variant="primary" disabled={openSession.isPending}>
            {t("cash:open.submit")}
          </Button>
        </>
      }
    >
      <form id="open-session" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <Field
          id="float"
          label={t("cash:open.float")}
          error={invalid ? t("validation:amountFormat") : undefined}
        >
          <NumericInput
            id="float"
            autoFocus
            value={amount}
            onValueChange={(v) => {
              setAmount(v);
              setInvalid(false);
            }}
          />
        </Field>
        {counting ? (
          <CountSheet
            counts={counts}
            onChange={(next) => {
              setCounts(next);
              setAmount(formatAmountInput(countedTotal(next), lang));
            }}
          />
        ) : (
          <Button variant="outline" onClick={() => setCounting(true)}>
            {t("cash:open.count")}
          </Button>
        )}
        {openSession.error ? (
          <p role="alert" className="text-danger">
            {errorText(openSession.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
