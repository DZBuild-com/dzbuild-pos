import { Plus, Trash2 } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { NativeSelect } from "@/components/ui/native-select";
import { useCan } from "@/features/stock/stock-parts";
import { formatAmountInput } from "@/lib/format";
import { usePriceLevels } from "@/lib/ipc/catalog";
import { toAppError } from "@/lib/ipc/client";
import { usePriceBreaks, useReplacePriceBreaks } from "@/lib/ipc/product-extras";
import type { PriceBreak, PriceBreakInput, PriceLevel, Product } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty, parseAmount, parseQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

interface Row {
  key: number;
  priceLevelId: string;
  minQty: string;
  price: string;
}

/** `breaks.2.price` → 2: the row a backend refusal names. */
function refusedRow(field: string | null | undefined): number | null {
  const match = /^breaks\.(\d+)\./.exec(field ?? "");
  return match ? Number(match[1]) : null;
}

/**
 * PRC-004: quantity breaks per price level, "from N units: this unit price" (a carton of water
 * cheaper per bottle). The cart takes the best break for the line's quantity. Saving replaces the
 * product's whole list; changing it needs `products.price` alone, as `products_breaks_replace` does
 * (M2 permission split: prices apart from `products.edit`).
 */
export function PriceBreaksEditor({ product }: { product: Product }) {
  const breaks = usePriceBreaks(product.id);
  const levels = usePriceLevels();
  const errorText = useErrorText();
  const error = breaks.error ?? levels.error;
  if (error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(error)}
      </p>
    );
  }
  if (!breaks.data || !levels.data) return null;
  return (
    <BreaksForm
      key={product.id}
      product={product}
      initial={breaks.data}
      levels={levels.data.filter((l) => !l.archived)}
    />
  );
}

function BreaksForm({
  product,
  initial,
  levels,
}: {
  product: Product;
  initial: PriceBreak[];
  levels: PriceLevel[];
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const can = useCan();
  const id = useId();
  const replace = useReplacePriceBreaks();
  const mayEdit = can("products.price");
  const defaultLevel = levels.find((l) => l.isDefault)?.id ?? levels[0]?.id ?? "";
  const [rows, setRows] = useState<Row[]>(() =>
    initial.map((b, i) => ({
      key: i,
      priceLevelId: b.priceLevelId,
      minQty: formatQty(b.minQty, lang),
      price: formatAmountInput(b.price, lang),
    })),
  );
  const [nextKey, setNextKey] = useState(initial.length);
  const [problems, setProblems] = useState<Record<number, string>>({});
  const [failure, setFailure] = useState<unknown>(null);

  const patch = (key: number, change: Partial<Row>) =>
    setRows((all) => all.map((r) => (r.key === key ? { ...r, ...change } : r)));
  const add = () => {
    setRows((all) => [...all, { key: nextKey, priceLevelId: defaultLevel, minQty: "", price: "" }]);
    setNextKey((k) => k + 1);
  };
  const remove = (key: number) => {
    setRows((all) => all.filter((r) => r.key !== key));
    // The problems name rows by position, which the removal shifts.
    setProblems({});
  };

  const save = () => {
    const found: Record<number, string> = {};
    const list: PriceBreakInput[] = [];
    rows.forEach((row, index) => {
      const minQty = parseQty(row.minQty, product.unitDecimals);
      const price = parseAmount(row.price);
      if (minQty === null) found[index] = t("validation:qtyFormat");
      else if (price === null) found[index] = t("validation:amountFormat");
      else list.push({ priceLevelId: row.priceLevelId, minQty, price });
    });
    setProblems(found);
    setFailure(null);
    if (Object.keys(found).length > 0) return;
    replace.mutate(
      { id: product.id, breaks: list },
      {
        onSuccess: () => toast.success(t("products:breaks.saved")),
        onError: (error) => {
          const row = refusedRow(toAppError(error).field);
          if (row === null) setFailure(error);
          else setProblems({ [row]: errorText(error) });
        },
      },
    );
  };

  return (
    <section className="flex flex-col gap-4">
      <p className="text-ink-2">{t("products:breaks.hint")}</p>
      {rows.length === 0 ? (
        <p className="text-ink-2">{t("products:breaks.empty")}</p>
      ) : (
        <div className="flex flex-col gap-2">
          <div
            aria-hidden
            className="grid grid-cols-[1fr_1fr_1fr_2.75rem] gap-3 text-sm font-semibold text-ink-2"
          >
            <span>{t("products:breaks.level")}</span>
            <span>{t("products:breaks.minQty")}</span>
            <span>{t("products:breaks.price")}</span>
          </div>
          {rows.map((row, index) => {
            const n = index + 1;
            const problem = problems[index];
            return (
              <div key={row.key} className="flex flex-col gap-1">
                <div className="grid grid-cols-[1fr_1fr_1fr_2.75rem] items-center gap-3">
                  <NativeSelect
                    aria-label={t("products:breaks.levelOf", { n })}
                    disabled={!mayEdit}
                    value={row.priceLevelId}
                    onChange={(e) => patch(row.key, { priceLevelId: e.target.value })}
                  >
                    {levels.map((l) => (
                      <option key={l.id} value={l.id}>
                        {localizedName(l, lang)}
                      </option>
                    ))}
                  </NativeSelect>
                  <NumericInput
                    kind="qty"
                    aria-label={t("products:breaks.minQtyOf", { n })}
                    aria-invalid={Boolean(problem)}
                    aria-describedby={problem ? `${id}-${row.key}` : undefined}
                    disabled={!mayEdit}
                    value={row.minQty}
                    onValueChange={(minQty) => patch(row.key, { minQty })}
                  />
                  <NumericInput
                    aria-label={t("products:breaks.priceOf", { n })}
                    aria-invalid={Boolean(problem)}
                    aria-describedby={problem ? `${id}-${row.key}` : undefined}
                    disabled={!mayEdit}
                    value={row.price}
                    onValueChange={(price) => patch(row.key, { price })}
                  />
                  {mayEdit ? (
                    <Button
                      variant="ghost"
                      size="icon"
                      aria-label={t("products:breaks.remove", { n })}
                      title={t("products:breaks.remove", { n })}
                      onClick={() => remove(row.key)}
                    >
                      <Trash2 aria-hidden />
                    </Button>
                  ) : null}
                </div>
                {problem ? (
                  <p id={`${id}-${row.key}`} role="alert" className="text-sm text-danger">
                    {problem}
                  </p>
                ) : null}
              </div>
            );
          })}
        </div>
      )}
      {failure ? (
        <p role="alert" className="text-danger">
          {errorText(failure)}
        </p>
      ) : null}
      {mayEdit ? (
        <div className="flex flex-wrap justify-between gap-2">
          <Button variant="outline" onClick={add}>
            <Plus aria-hidden />
            {t("products:breaks.add")}
          </Button>
          <Button variant="primary" disabled={replace.isPending} onClick={save}>
            {t("products:breaks.save")}
          </Button>
        </div>
      ) : null}
    </section>
  );
}
