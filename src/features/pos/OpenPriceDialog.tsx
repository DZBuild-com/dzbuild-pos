import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { parseAmount, parseQty } from "@/lib/numeric";

/**
 * A product priced when it is sold (a department key, a top-up, an open-price service, SAL-019,
 * VRT-002): the till asks for the price and the quantity before the line enters the cart. Without
 * its price the backend refuses to price the line, so the sale could neither be paid nor voided.
 */
export function OpenPriceDialog({
  title,
  unitDecimals,
  onAdd,
  onClose,
}: {
  title: string;
  /** The quantity's decimals, as the till adds that product's lines. */
  unitDecimals: number;
  onAdd: (unitPrice: number, qty: number) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [price, setPrice] = useState("");
  const [qty, setQty] = useState("1");
  const [errors, setErrors] = useState<Record<string, string>>({});

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const unitPrice = parseAmount(price);
    const methyl = parseQty(qty.trim() === "" ? "1" : qty, unitDecimals);
    const found: Record<string, string> = {};
    if (unitPrice === null || unitPrice <= 0) found.price = t("validation:amountFormat");
    if (methyl === null || methyl <= 0) found.qty = t("validation:qtyFormat");
    setErrors(found);
    if (Object.keys(found).length > 0 || unitPrice === null || methyl === null) return;
    onAdd(unitPrice, methyl);
    onClose();
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={title}
      description={t("pos:openPrice.hint")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="open-price" variant="primary">
            {t("pos:openPrice.add")}
          </Button>
        </>
      }
    >
      <form id="open-price" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <Field id="open-price-price" label={t("pos:openPrice.price")} error={errors.price}>
          <NumericInput id="open-price-price" autoFocus value={price} onValueChange={setPrice} />
        </Field>
        <Field id="open-price-qty" label={t("pos:openPrice.qty")} error={errors.qty}>
          <NumericInput id="open-price-qty" kind="qty" value={qty} onValueChange={setQty} />
        </Field>
      </form>
    </Dialog>
  );
}
