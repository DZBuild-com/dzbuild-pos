import { type FormEvent, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formatAmountInput, formatPercentInput, parsePercentBp } from "@/lib/format";
import { formatQty, parseAmount, parseQty } from "@/lib/numeric";
import { useLang } from "@/lib/use-language";
import { useCart } from "./cart-store";
import { NamedDiscounts } from "./NamedDiscounts";
import type { PosContext } from "./PosDialogs";

/** Tap a line (spec §10.5): quantity, price override, discount (% or DA, or a named one in one
 * tap), note, or void it. The dialog follows the line it was opened for by its key, not by its
 * place in the ticket: once that line is gone it closes, and never shows the next one's name. */
export function LineEditDialog({
  index,
  ctx,
  onClose,
}: {
  index: number;
  ctx: PosContext;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const [key] = useState(() => useCart.getState().lines[index]?.key ?? null);
  const draft = useCart((s) => s.lines.find((l) => l.key === key));
  const cart = useCart.getState();
  /** Where the line is in the ticket right now (-1 once it is gone). */
  const position = () => useCart.getState().lines.findIndex((l) => l.key === key);
  const decimals = draft?.display.unitDecimals ?? 0;
  const [qty, setQty] = useState(draft ? formatQty(draft.line.qty, lang) : "");
  const [price, setPrice] = useState(
    draft?.line.unitPrice != null ? formatAmountInput(draft.line.unitPrice, lang) : "",
  );
  const initial = draft?.line.discount;
  const [kind, setKind] = useState<"percent" | "amount">(initial?.kind === "amount" ? "amount" : "percent");
  const [discount, setDiscount] = useState(
    initial?.kind === "percent"
      ? formatPercentInput(initial.value, lang)
      : initial?.kind === "amount"
        ? formatAmountInput(initial.value, lang)
        : "",
  );
  const [note, setNote] = useState(draft?.line.note ?? "");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [removing, setRemoving] = useState(false);
  const gone = draft === undefined;
  useEffect(() => {
    if (gone) onClose();
  }, [gone, onClose]);
  if (!draft) return null;
  /** Saves the line as typed; `namedBp` is a named discount tapped instead of the typed one. */
  const save = async (namedBp?: number) => {
    const found: Record<string, string> = {};
    const parsedQty = parseQty(qty, decimals);
    if (parsedQty === null) found.qty = t("validation:qtyFormat");
    const parsedPrice = price.trim() === "" ? null : parseAmount(price);
    if (price.trim() !== "" && parsedPrice === null) found.price = t("validation:amountFormat");
    const discountKind = namedBp === undefined ? kind : "percent";
    const parsedDiscount =
      namedBp !== undefined
        ? namedBp
        : discount.trim() === ""
          ? 0
          : kind === "percent"
            ? parsePercentBp(discount)
            : parseAmount(discount);
    if (parsedDiscount === null)
      found.discount = t(kind === "percent" ? "validation:percentRange" : "validation:amountFormat");
    setErrors(found);
    if (Object.keys(found).length > 0 || parsedQty === null || parsedDiscount === null) return;
    if (position() < 0) return;
    // Fewer units than rung is a partial void: journaled (and approved when needed) first.
    if (parsedQty < draft.line.qty) {
      setSaving(true);
      const reduced = await ctx.reduceQty(position(), parsedQty);
      setSaving(false);
      if (!reduced) return;
    } else {
      cart.setQty(position(), parsedQty);
    }
    const at = position();
    if (at < 0) return;
    cart.setUnitPrice(at, parsedPrice);
    cart.setDiscount(
      at,
      parsedDiscount === 0 ? { kind: "none" } : { kind: discountKind, value: parsedDiscount },
    );
    cart.setNote(at, note.trim() === "" ? null : note.trim());
    onClose();
  };
  /** Voids the line (journaled, a manager's approval when needed). The dialog stays open, with
   * what was typed, when the approval is cancelled or the void refused. */
  const remove = async () => {
    const at = position();
    if (at < 0 || removing) return;
    setRemoving(true);
    const removed = await ctx.voidLine(at);
    if (removed) onClose();
    else setRemoving(false);
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    void save();
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("pos:edit.title")}
      description={draft.display.name}
      footer={
        <>
          <Button
            variant="danger"
            className="me-auto"
            disabled={removing || saving}
            aria-busy={removing || undefined}
            onClick={() => void remove()}
          >
            {t("pos:edit.void")}
          </Button>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="line-edit" variant="primary" disabled={saving || removing}>
            {t("pos:edit.save")}
          </Button>
        </>
      }
    >
      <form id="line-edit" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <Field id="line-qty" label={t("pos:edit.qty")} error={errors.qty}>
          <NumericInput id="line-qty" kind="qty" autoFocus value={qty} onValueChange={setQty} />
        </Field>
        <Field
          id="line-price"
          label={t("pos:edit.unitPrice")}
          hint={t("pos:edit.unitPriceHint")}
          error={errors.price}
        >
          <NumericInput id="line-price" value={price} onValueChange={setPrice} />
        </Field>
        <RadioCards
          legend={t("pos:discount.kind")}
          name="line-discount-kind"
          value={kind}
          onChange={setKind}
          options={[
            { value: "percent", label: t("pos:discount.percent") },
            { value: "amount", label: t("pos:discount.amount") },
          ]}
        />
        <Field id="line-discount" label={t("pos:discount.value")} error={errors.discount}>
          <NumericInput id="line-discount" value={discount} onValueChange={setDiscount} />
        </Field>
        <NamedDiscounts onPick={(bp) => void save(bp)} />
        <Field id="line-note" label={t("pos:edit.note")} optional={t("setup:shop.optional")}>
          <Input id="line-note" value={note} maxLength={200} onChange={(e) => setNote(e.target.value)} />
        </Field>
      </form>
    </Dialog>
  );
}
