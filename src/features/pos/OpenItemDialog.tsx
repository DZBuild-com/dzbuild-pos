import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { useTaxRates } from "@/lib/ipc/catalog";
import { localizedName } from "@/lib/names";
import { parseAmount, parseQty } from "@/lib/numeric";
import { useLang } from "@/lib/use-language";
import { useCart } from "./cart-store";

type OpenItem = { description: string; unitPrice: number; taxRateId: string; qty: number };

/** SAL-018: an article without a catalogue entry; the backend asks for `sales.price_override`.
 * It goes into the till's cart, or to `onAdd` (the document editor's lines). */
export function OpenItemDialog({
  onClose,
  onAdd,
}: {
  onClose: () => void;
  onAdd?: (item: OpenItem) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const rates = (useTaxRates().data ?? []).filter((r) => r.active);
  const [description, setDescription] = useState("");
  const [price, setPrice] = useState("");
  const [qty, setQty] = useState("1");
  const [rateId, setRateId] = useState<string>("");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const unitPrice = parseAmount(price);
    const quantity = parseQty(qty, 3);
    const taxRateId = rateId || rates[0]?.id || "";
    const found: Record<string, string> = {};
    if (description.trim() === "") found.description = t("validation:required");
    if (unitPrice === null || unitPrice <= 0) found.price = t("validation:amountFormat");
    if (quantity === null) found.qty = t("validation:qtyFormat");
    setErrors(found);
    if (Object.keys(found).length > 0 || unitPrice === null || quantity === null || taxRateId === "") return;
    const item = { description: description.trim(), unitPrice, taxRateId, qty: quantity };
    if (onAdd) onAdd(item);
    else useCart.getState().addOpenItem(item);
    onClose();
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("pos:openItem.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="open-item" variant="primary">
            {t("pos:openItem.submit")}
          </Button>
        </>
      }
    >
      <form id="open-item" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <Field id="open-desc" label={t("pos:openItem.description")} error={errors.description}>
          <Input
            id="open-desc"
            dir="auto"
            autoFocus
            maxLength={120}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </Field>
        <Field id="open-price" label={t("pos:openItem.price")} error={errors.price}>
          <NumericInput id="open-price" value={price} onValueChange={setPrice} />
        </Field>
        <Field id="open-qty" label={t("pos:edit.qty")} error={errors.qty}>
          <NumericInput id="open-qty" kind="qty" value={qty} onValueChange={setQty} />
        </Field>
        <Field id="open-rate" label={t("pos:openItem.taxRate")}>
          <NativeSelect
            id="open-rate"
            value={rateId || rates[0]?.id || ""}
            onChange={(e) => setRateId(e.target.value)}
          >
            {rates.map((r) => (
              <option key={r.id} value={r.id}>
                {localizedName(r, lang)}
              </option>
            ))}
          </NativeSelect>
        </Field>
      </form>
    </Dialog>
  );
}
