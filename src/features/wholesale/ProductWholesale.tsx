import { Package } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { useUnits } from "@/lib/ipc/catalog";
import { useModule } from "@/lib/ipc/pack-state";
import type { Product, Session } from "@/lib/ipc/types";
import { useMinOrder, useSetMinOrder } from "@/lib/ipc/wholesale";
import { localizedName } from "@/lib/names";
import { formatQty, parseQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { PackDialog } from "./PackDialog";

/**
 * The product form's wholesale part, while the wholesale module is on: the minimum order (WHS,
 * quantity in the product's unit, empty = none; saved when the field is left, `products.price`)
 * and, on a simple product, "Créer un pack" (PRD-033, `products.edit`).
 */
export function ProductWholesale({ product, session }: { product: Product; session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const on = useModule("wholesale");
  const [packing, setPacking] = useState(false);
  const can = (p: Session["permissions"][number]) => session.permissions.includes(p);
  const unit = useUnits().data?.find((u) => u.id === product.unitId);
  if (!on) return null;
  return (
    <section aria-labelledby="product-wholesale" className="flex flex-col gap-3 border-t border-line pt-4">
      <h3 id="product-wholesale" className="font-bold">
        {t("wholesale:section")}
      </h3>
      <div className="flex flex-wrap items-end gap-4">
        <MinOrderField
          product={product}
          unitName={unit ? localizedName(unit, lang) : product.unitCode}
          editable={can("products.price") && !product.archived}
        />
        {product.kind === "simple" && can("products.edit") && !product.archived ? (
          <Button variant="outline" onClick={() => setPacking(true)}>
            <Package aria-hidden />
            {t("wholesale:pack.create")}
          </Button>
        ) : null}
      </div>
      {packing ? <PackDialog product={product} onClose={() => setPacking(false)} /> : null}
    </section>
  );
}

function MinOrderField({
  product,
  unitName,
  editable,
}: {
  product: Product;
  unitName: string;
  editable: boolean;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const stored = useMinOrder(product.id);
  const save = useSetMinOrder();
  const [typed, setTyped] = useState<string | null>(null);
  const [invalid, setInvalid] = useState(false);
  const shown = stored.data === undefined || stored.data === null ? "" : formatQty(stored.data, lang);
  const value = typed ?? shown;
  const commit = () => {
    if (typed === null || typed.trim() === shown) {
      setTyped(null);
      return;
    }
    const qty = typed.trim() === "" ? null : parseQty(typed, product.unitDecimals);
    if (typed.trim() !== "" && qty === null) {
      setInvalid(true);
      return;
    }
    save.mutate(
      { productId: product.id, minOrderQty: qty },
      {
        onSuccess: () => {
          setTyped(null);
          toast.success(t("wholesale:minOrderSaved"));
        },
      },
    );
  };
  return (
    <div className="w-60">
      <Field
        id="product-min-order"
        label={t("wholesale:minOrder")}
        hint={t("wholesale:minOrderHint", { unit: unitName })}
        error={invalid ? t("validation:qtyFormat") : save.error ? errorText(save.error) : undefined}
      >
        <NumericInput
          id="product-min-order"
          kind={product.unitDecimals > 0 ? "qty" : "integer"}
          disabled={!editable || stored.isPending || save.isPending}
          aria-invalid={invalid}
          value={value}
          onValueChange={(v) => {
            setTyped(v);
            setInvalid(false);
          }}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              event.currentTarget.blur();
            }
          }}
        />
      </Field>
    </div>
  );
}
