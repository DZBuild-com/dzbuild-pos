import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { useUnitDecimals } from "@/features/purchases/purchase-lines";
import { useMinimums, useSetMinimums } from "@/lib/ipc/stock-m2";
import type { LocationMinimum } from "@/lib/ipc/types";
import { formatQty, parseQtyOrZero } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { useLocations } from "./stock-parts";

/** STK-014: a product's minimum stock at each storage location (the reorder point of the low-stock
 * alerts). Empty or 0 removes the location's own minimum. */
export function MinimumsDialog({
  productId,
  name,
  unitCode,
  onClose,
}: {
  productId: string;
  name: string;
  unitCode: string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const minimums = useMinimums(productId);
  return (
    <Dialog
      open
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("stock:minimums.title", { name })}
    >
      {minimums.data ? (
        <MinimumsForm productId={productId} unitCode={unitCode} saved={minimums.data} onClose={onClose} />
      ) : (
        <p className="text-ink-2">{t("common:status.loading")}</p>
      )}
    </Dialog>
  );
}

function MinimumsForm({
  productId,
  unitCode,
  saved,
  onClose,
}: {
  productId: string;
  unitCode: string;
  saved: LocationMinimum[];
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const decimalsOf = useUnitDecimals();
  const { active, nameOf } = useLocations();
  const set = useSetMinimums();
  const storage = active.filter((l) => l.kind === "storage");
  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(saved.map((m) => [m.locationId, formatQty(m.minQty, lang)])),
  );
  const [invalid, setInvalid] = useState<string[]>([]);

  const submit = () => {
    const decimals = decimalsOf(unitCode);
    const wrong: string[] = [];
    const minimums: LocationMinimum[] = [];
    for (const location of storage) {
      const text = (values[location.id] ?? "").trim();
      if (text === "") continue;
      const minQty = parseQtyOrZero(text, decimals);
      if (minQty === null) wrong.push(location.id);
      else if (minQty > 0) minimums.push({ locationId: location.id, minQty });
    }
    setInvalid(wrong);
    if (wrong.length > 0) return;
    set.mutate(
      { productId, minimums },
      {
        onSuccess: () => {
          toast.success(t("stock:minimums.saved"));
          onClose();
        },
      },
    );
  };

  return (
    <form
      noValidate
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <p className="text-sm text-ink-2">{t("stock:minimums.hint")}</p>
      {storage.map((location) => (
        <Field
          key={location.id}
          id={`minimum-${location.id}`}
          label={nameOf(location.id)}
          error={invalid.includes(location.id) ? t("validation:qtyFormat") : undefined}
        >
          <NumericInput
            id={`minimum-${location.id}`}
            kind="qty"
            aria-label={t("stock:minimums.fieldOf", { location: nameOf(location.id) })}
            aria-invalid={invalid.includes(location.id)}
            value={values[location.id] ?? ""}
            onValueChange={(value) => setValues((all) => ({ ...all, [location.id]: value }))}
          />
        </Field>
      ))}
      {set.error ? (
        <p role="alert" className="text-danger">
          {errorText(set.error)}
        </p>
      ) : null}
      <div className="flex flex-wrap justify-end gap-2">
        <Button variant="outline" onClick={onClose}>
          {t("common:actions.cancel")}
        </Button>
        <Button type="submit" variant="primary" disabled={set.isPending}>
          {t("stock:minimums.save")}
        </Button>
      </div>
    </form>
  );
}
