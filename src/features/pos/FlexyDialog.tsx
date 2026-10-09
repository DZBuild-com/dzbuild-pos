import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { formatDZD } from "@/lib/format";
import { flexyLine, useFlexyOperators } from "@/lib/ipc/till-packs";
import { localizedName } from "@/lib/names";
import { normalizeDigits, parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { useTillKit } from "./till-kit";

/**
 * VRT-002 (Ctrl+2): a mobile top-up. The operator is guessed from the number's prefix and can be
 * changed; its float is shown; "Ajouter la recharge" adds the backend's top-up line (`flexy_line`)
 * to the ticket, sold like any line (the float moves with the sale).
 */
export function FlexyDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const { useCart } = useTillKit();
  const operators = useFlexyOperators();
  const [phone, setPhone] = useState("");
  const [picked, setPicked] = useState<string | null>(null);
  const [amount, setAmount] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const digits = normalizeDigits(phone).replace(/\D/g, "");
  const list = operators.data ?? [];
  const guessed = list.find((o) => o.prefixes.some((p) => digits.startsWith(p)))?.code ?? null;
  const operator = list.find((o) => o.code === (picked ?? guessed)) ?? null;
  const cents = parseAmount(amount);
  const ready = operator !== null && digits.length >= 9 && cents !== null && cents > 0 && !busy;

  const submit = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!ready) return;
    setBusy(true);
    setError(null);
    try {
      const line = await flexyLine(operator.code, digits, cents);
      useCart.getState().addLine(line, {
        name: `${localizedName(operator, lang)} ${digits}`,
        unitCode: "pc",
        unitDecimals: 0,
        price: cents,
      });
      onClose();
    } catch (e) {
      setError(e);
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("packs:till.flexy.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={!ready} onClick={() => void submit()}>
            {t("packs:till.flexy.submit")}
          </Button>
        </>
      }
    >
      <form noValidate onSubmit={(event) => void submit(event)} className="flex flex-col gap-4">
        <Field id="flexy-phone" label={t("packs:till.flexy.phone")}>
          <NumericInput
            id="flexy-phone"
            kind="integer"
            autoFocus
            maxLength={10}
            className="min-h-14 text-xl"
            value={phone}
            onValueChange={setPhone}
          />
        </Field>
        {list.length > 0 ? (
          <RadioCards
            legend={t("packs:till.flexy.operator")}
            name="flexy-operator"
            value={operator?.code ?? ""}
            onChange={setPicked}
            options={list.map((o) => ({
              value: o.code,
              label: localizedName(o, lang),
              hint: t("packs:till.flexy.float", {
                operator: localizedName(o, lang),
                amount: formatDZD(o.balance, lang),
              }),
            }))}
          />
        ) : null}
        <Field id="flexy-amount" label={t("packs:till.flexy.amount")}>
          <NumericInput
            id="flexy-amount"
            className="min-h-14 text-xl"
            value={amount}
            onValueChange={setAmount}
          />
        </Field>
        {error || operators.error ? (
          <p role="alert" className="text-danger">
            {errorText(error ?? operators.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
