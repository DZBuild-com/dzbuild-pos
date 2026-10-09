import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useCan } from "@/features/stock/stock-parts";
import { useGenerateVariants } from "@/lib/ipc/product-extras";
import { useVariants } from "@/lib/ipc/products";
import type { Product, VariantAxis } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const MAX_AXES = 3;

interface AxisDraft {
  name: string;
  values: string;
}

/** "S, M, L" (or with the Arabic comma "،", or ";") → ["S", "M", "L"], blanks and repeats dropped. */
function splitValues(text: string): string[] {
  const values = text
    .split(/[,،;]/)
    .map((v) => v.trim())
    .filter((v) => v !== "");
  return [...new Set(values)];
}

/**
 * PRD-020: the variants matrix of a model. One to three criteria (size, colour, season) with their
 * values; every combination that does not exist yet becomes a variant with its reference, price,
 * cost and, on demand, an in-store EAN-13. The backend skips the existing combinations, so the
 * matrix can be run again to add a size or a colour.
 */
export function VariantsDialog({ product, onClose }: { product: Product; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const can = useCan();
  const generate = useGenerateVariants();
  const [axes, setAxes] = useState<AxisDraft[]>([{ name: "", values: "" }]);
  const [sellPrice, setSellPrice] = useState("");
  const [costPrice, setCostPrice] = useState("");
  const [internalBarcodes, setInternalBarcodes] = useState(false);
  const [invalid, setInvalid] = useState<"sellPrice" | "costPrice" | null>(null);

  const cleaned: VariantAxis[] = axes.map((a) => ({ name: a.name.trim(), values: splitValues(a.values) }));
  const combinations = cleaned.reduce((n, a) => n * a.values.length, 1);
  const ready = cleaned.every((a) => a.name !== "" && a.values.length > 0);
  const patch = (index: number, change: Partial<AxisDraft>) =>
    setAxes((all) => all.map((a, i) => (i === index ? { ...a, ...change } : a)));

  const submit = () => {
    const sell = sellPrice.trim() === "" ? null : parseAmount(sellPrice);
    const cost = costPrice.trim() === "" ? null : parseAmount(costPrice);
    if (sellPrice.trim() !== "" && sell === null) return setInvalid("sellPrice");
    if (costPrice.trim() !== "" && cost === null) return setInvalid("costPrice");
    setInvalid(null);
    generate.mutate(
      { parentId: product.id, input: { axes: cleaned, sellPrice: sell, costPrice: cost, internalBarcodes } },
      {
        onSuccess: (made) => {
          toast.success(t("products:variants.created", { count: made.length }));
          onClose();
        },
      },
    );
  };

  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("products:variants.title", { name: localizedName(product, lang) })}
      description={t("products:variants.hint")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={!ready || generate.isPending} onClick={submit}>
            {t("products:variants.create")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        {axes.map((axis, index) => {
          const n = index + 1;
          return (
            <div key={n} className="grid items-end gap-3 sm:grid-cols-[1fr_2fr_2.75rem]">
              <Field id={`variant-axis-${n}`} label={t("products:variants.axis", { n })}>
                <Input
                  id={`variant-axis-${n}`}
                  dir="auto"
                  value={axis.name}
                  onChange={(e) => patch(index, { name: e.target.value })}
                />
              </Field>
              <Field
                id={`variant-values-${n}`}
                label={t("products:variants.values", { n })}
                hint={t("products:variants.valuesHint")}
              >
                <Input
                  id={`variant-values-${n}`}
                  dir="auto"
                  value={axis.values}
                  onChange={(e) => patch(index, { values: e.target.value })}
                />
              </Field>
              {axes.length > 1 ? (
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={t("products:variants.removeAxis", { n })}
                  title={t("products:variants.removeAxis", { n })}
                  onClick={() => setAxes((all) => all.filter((_, i) => i !== index))}
                >
                  <Trash2 aria-hidden />
                </Button>
              ) : null}
            </div>
          );
        })}
        <div className="flex flex-wrap items-center justify-between gap-3">
          <Button
            variant="outline"
            disabled={axes.length >= MAX_AXES}
            onClick={() => setAxes((all) => [...all, { name: "", values: "" }])}
          >
            <Plus aria-hidden />
            {t("products:variants.addAxis")}
          </Button>
          <p className="font-semibold tabular-nums">
            {t("products:variants.combinations", { count: combinations })}
          </p>
        </div>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field
            id="variant-sell"
            label={t("products:variants.sellPrice")}
            hint={t("products:variants.priceHint")}
            error={invalid === "sellPrice" ? t("validation:amountFormat") : undefined}
          >
            <NumericInput id="variant-sell" value={sellPrice} onValueChange={setSellPrice} />
          </Field>
          {can("reports.profit") ? (
            <Field
              id="variant-cost"
              label={t("products:variants.costPrice")}
              hint={t("products:variants.priceHint")}
              error={invalid === "costPrice" ? t("validation:amountFormat") : undefined}
            >
              <NumericInput id="variant-cost" value={costPrice} onValueChange={setCostPrice} />
            </Field>
          ) : null}
        </div>
        <label className="flex items-center gap-3 font-semibold">
          <Checkbox checked={internalBarcodes} onCheckedChange={(v) => setInternalBarcodes(v === true)} />
          {t("products:variants.internalBarcodes")}
        </label>
        {generate.error ? (
          <p role="alert" className="text-danger">
            {errorText(generate.error)}
          </p>
        ) : null}
      </div>
    </Dialog>
  );
}

/** The variants tab of a model: its variants (at the till like any product) and the matrix. */
export function VariantsPanel({
  product,
  editable,
  onGenerate,
}: {
  product: Product;
  editable: boolean;
  onGenerate: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const variants = useVariants(product.id);
  const list = variants.data ?? [];
  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h3 className="text-lg font-bold">{t("products:variants.list")}</h3>
        {editable ? (
          <Button variant="outline" onClick={onGenerate}>
            <Plus aria-hidden />
            {t("products:variants.open")}
          </Button>
        ) : null}
      </div>
      {variants.error ? (
        <p role="alert" className="text-danger">
          {errorText(variants.error)}
        </p>
      ) : variants.data && list.length === 0 ? (
        <p className="text-ink-2">{t("products:variants.empty")}</p>
      ) : (
        <ul className="divide-y divide-line rounded-card border border-line">
          {list.map((v) => (
            <li key={v.id} className="flex min-h-11 items-center justify-between gap-3 px-3 py-2">
              <span className="flex flex-col">
                <span className="font-semibold">{localizedName(v, lang)}</span>
                {v.reference ? (
                  <bdi dir="ltr" className="text-sm text-ink-2">
                    {v.reference}
                  </bdi>
                ) : null}
              </span>
              <Amount value={v.sellPrice} />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
