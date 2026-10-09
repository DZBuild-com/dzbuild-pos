import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { parsePercentBp } from "@/lib/format";
import { parseAmount } from "@/lib/numeric";
import { useCart } from "./cart-store";

/** F5 (PRM-001/002): a discount on the selected line (% or DA) or on every line (%). Limits and
 * approvals are the backend's: an excess shows in the cart with "ask a manager". */
export function DiscountDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const selected = useCart((s) => s.selected);
  const hasLines = useCart((s) => s.lines.length > 0);
  const [scope, setScope] = useState<"line" | "all">(selected === null ? "all" : "line");
  const [kind, setKind] = useState<"percent" | "amount">("percent");
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const cart = useCart.getState();
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const effectiveKind = scope === "all" ? "percent" : kind;
    const parsed = effectiveKind === "percent" ? parsePercentBp(value) : parseAmount(value);
    if (parsed === null || parsed <= 0) {
      setError(t(effectiveKind === "percent" ? "validation:percentRange" : "validation:amountFormat"));
      return;
    }
    const discount = { kind: effectiveKind, value: parsed } as const;
    if (scope === "all") cart.discountAll(discount);
    else if (selected !== null) cart.setDiscount(selected, discount);
    onClose();
  };
  const clear = () => {
    if (scope === "all") cart.discountAll({ kind: "none" });
    else if (selected !== null) cart.setDiscount(selected, { kind: "none" });
    onClose();
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("pos:discount.title")}
      footer={
        <>
          <Button variant="outline" className="me-auto" onClick={clear} disabled={!hasLines}>
            {t("pos:discount.remove")}
          </Button>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="discount" variant="primary" disabled={!hasLines}>
            {t("pos:discount.apply")}
          </Button>
        </>
      }
    >
      <form id="discount" noValidate onSubmit={submit} className="flex flex-col gap-4">
        {!hasLines ? <p className="text-ink-2">{t("pos:cart.empty")}</p> : null}
        <RadioCards
          legend={t("pos:discount.scope")}
          name="discount-scope"
          value={scope}
          onChange={setScope}
          options={[
            ...(selected !== null ? [{ value: "line" as const, label: t("pos:discount.scopeLine") }] : []),
            {
              value: "all" as const,
              label: t("pos:discount.scopeAll"),
              hint: t("pos:discount.allPercentOnly"),
            },
          ]}
        />
        {scope === "line" ? (
          <RadioCards
            legend={t("pos:discount.kind")}
            name="discount-kind"
            value={kind}
            onChange={setKind}
            options={[
              { value: "percent", label: t("pos:discount.percent") },
              { value: "amount", label: t("pos:discount.amount") },
            ]}
          />
        ) : null}
        <Field id="discount-value" label={t("pos:discount.value")} error={error ?? undefined}>
          <NumericInput
            id="discount-value"
            autoFocus
            value={value}
            onValueChange={(v) => {
              setValue(v);
              setError(null);
            }}
          />
        </Field>
      </form>
    </Dialog>
  );
}
