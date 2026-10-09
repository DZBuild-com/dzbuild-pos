import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { useCategories, useTaxRates } from "@/lib/ipc/catalog";
import { useBulkEdit } from "@/lib/ipc/product-extras";
import type { BulkEdit } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { parseQtyOrZero } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

type TillChoice = "" | "yes" | "no";

/**
 * PRD-066/070: one change on every selected product — the category, the VAT rate, whether it is
 * sold at the till, the minimum stock. A field left at "Ne pas changer" is sent as `null` and the
 * backend keeps each product's own value. Prices change in `BulkPriceDialog` (with its preview).
 */
export function BulkEditDialog({ productIds, onClose }: { productIds: string[]; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const categories = (useCategories().data ?? []).filter((c) => !c.archived);
  const rates = (useTaxRates().data ?? []).filter((r) => r.active);
  const edit = useBulkEdit();
  const [categoryId, setCategoryId] = useState("");
  const [taxRateId, setTaxRateId] = useState("");
  const [till, setTill] = useState<TillChoice>("");
  const [minStock, setMinStock] = useState("");
  const [invalidMin, setInvalidMin] = useState(false);
  const nothing = categoryId === "" && taxRateId === "" && till === "" && minStock.trim() === "";

  const save = () => {
    const min = minStock.trim() === "" ? null : parseQtyOrZero(minStock, 3);
    setInvalidMin(minStock.trim() !== "" && min === null);
    if (minStock.trim() !== "" && min === null) return;
    const edits: BulkEdit[] = productIds.map((productId) => ({
      productId,
      sellPrice: null,
      costPrice: null,
      minStock: min,
      categoryId: categoryId === "" ? null : categoryId,
      taxRateId: taxRateId === "" ? null : taxRateId,
      availableAtTill: till === "" ? null : till === "yes",
    }));
    edit.mutate(edits, {
      onSuccess: (count) => {
        toast.success(t("products:bulkEdit.saved", { count }));
        onClose();
      },
    });
  };

  const unchanged = <option value="">{t("products:bulkEdit.unchanged")}</option>;
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("products:bulkEdit.title")}
      description={t("products:bulkEdit.hint")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={nothing || edit.isPending} onClick={save}>
            {t("products:bulkEdit.save")}
          </Button>
        </>
      }
    >
      <p className="font-semibold">{t("products:bulkPrice.selected", { count: productIds.length })}</p>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field id="bulk-edit-category" label={t("products:fields.category")}>
          <NativeSelect
            id="bulk-edit-category"
            value={categoryId}
            onChange={(e) => setCategoryId(e.target.value)}
          >
            {unchanged}
            {categories.map((c) => (
              <option key={c.id} value={c.id}>
                {localizedName(c, lang)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id="bulk-edit-rate" label={t("products:fields.taxRate")}>
          <NativeSelect id="bulk-edit-rate" value={taxRateId} onChange={(e) => setTaxRateId(e.target.value)}>
            {unchanged}
            {rates.map((r) => (
              <option key={r.id} value={r.id}>
                {localizedName(r, lang)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id="bulk-edit-till" label={t("products:fields.availableAtTill")}>
          <NativeSelect
            id="bulk-edit-till"
            value={till}
            onChange={(e) => setTill(e.target.value as TillChoice)}
          >
            {unchanged}
            <option value="yes">{t("products:bulkEdit.yes")}</option>
            <option value="no">{t("products:bulkEdit.no")}</option>
          </NativeSelect>
        </Field>
        <Field
          id="bulk-edit-min"
          label={t("products:fields.minStock")}
          hint={t("products:bulkEdit.minStockHint")}
          error={invalidMin ? t("validation:qtyFormat") : undefined}
        >
          <NumericInput
            id="bulk-edit-min"
            kind="qty"
            aria-invalid={invalidMin}
            value={minStock}
            onValueChange={setMinStock}
          />
        </Field>
      </div>
      {edit.error ? (
        <p role="alert" className="text-danger">
          {errorText(edit.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
