import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { useCan } from "@/features/stock/stock-parts";
import { parsePercentBp } from "@/lib/format";
import { useCategories, usePriceLevels } from "@/lib/ipc/catalog";
import { useApplyBulkPrice, useBulkPricePreview } from "@/lib/ipc/product-extras";
import { useSupplierSearch } from "@/lib/ipc/suppliers";
import type { BulkPriceInput, PriceChange, PriceChangeRow } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const CHANGES = ["percent_up", "percent_down", "amount_up", "amount_down", "set"] as const;
type ChangeChoice = (typeof CHANGES)[number];

/** Round to the nearest multiple of this many centimes (the backend rounds). */
const ROUNDINGS = [
  { value: 0, key: "none" },
  { value: 100, key: "dinar" },
  { value: 500, key: "five" },
  { value: 1_000, key: "ten" },
] as const;

interface Draft {
  categoryId: string;
  brand: string;
  supplierId: string;
  priceLevelId: string;
  change: ChangeChoice;
  value: string;
  roundTo: number;
}

/** The typed change → the backend's `PriceChange` (signed), or null while it cannot be read. */
function toChange(choice: ChangeChoice, value: string): PriceChange | null {
  if (choice === "percent_up" || choice === "percent_down") {
    const bp = parsePercentBp(value);
    if (bp === null || bp === 0) return null;
    return { kind: "percent", value: choice === "percent_up" ? bp : -bp };
  }
  const amount = parseAmount(value);
  if (amount === null) return null;
  if (choice === "set") return { kind: "set", value: amount };
  if (amount === 0) return null;
  return { kind: "amount", value: choice === "amount_up" ? amount : -amount };
}

const col = columnHelper<PriceChangeRow>();

/**
 * PRC-006/007: changes many prices at once, for the chosen products or for a category, a brand or
 * the usual supplier (every product when none is chosen), by a percentage, an amount or one price
 * for all, rounded to the dinar or to 5 or 10 DA. The new prices come from
 * `products_bulk_price_preview` and nothing changes before "Appliquer", which sends exactly what
 * was previewed; changing the form hides the preview.
 */
export function BulkPriceDialog({ productIds, onClose }: { productIds: string[]; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const can = useCan();
  const readsSuppliers = can("suppliers.manage") || can("purchases.view");
  const categories = (useCategories().data ?? []).filter((c) => !c.archived);
  const levels = (usePriceLevels().data ?? []).filter((l) => !l.archived);
  const suppliers = useSupplierSearch({}, readsSuppliers && productIds.length === 0).data ?? [];
  const [draft, setDraft] = useState<Draft>({
    categoryId: "",
    brand: "",
    supplierId: "",
    priceLevelId: "",
    change: "percent_up",
    value: "",
    roundTo: 0,
  });
  const [invalid, setInvalid] = useState(false);
  const [previewed, setPreviewed] = useState<BulkPriceInput | null>(null);
  const preview = useBulkPricePreview(previewed);
  const apply = useApplyBulkPrice();

  const update = (change: Partial<Draft>) => {
    setDraft((d) => ({ ...d, ...change }));
    setPreviewed(null);
  };
  const showPreview = () => {
    const change = toChange(draft.change, draft.value);
    setInvalid(change === null);
    if (!change) return;
    const byProducts = productIds.length > 0;
    setPreviewed({
      productIds,
      categoryId: byProducts || draft.categoryId === "" ? null : draft.categoryId,
      brand: byProducts || draft.brand.trim() === "" ? null : draft.brand.trim(),
      supplierId: byProducts || draft.supplierId === "" ? null : draft.supplierId,
      priceLevelId: draft.priceLevelId === "" ? null : draft.priceLevelId,
      change,
      roundTo: draft.roundTo,
    });
  };
  const run = () => {
    if (!previewed) return;
    apply.mutate(previewed, {
      onSuccess: (count) => {
        toast.success(t("products:bulkPrice.applied", { count }));
        onClose();
      },
    });
  };

  const columns = useMemo(
    () =>
      col.columns([
        col.accessor((r) => r.reference ?? "", {
          id: "reference",
          header: () => t("products:bulkPrice.columns.reference"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor((r) => localizedName(r, lang), {
          id: "product",
          header: () => t("products:bulkPrice.columns.product"),
        }),
        col.accessor("before", {
          header: () => t("products:bulkPrice.columns.before"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("after", {
          header: () => t("products:bulkPrice.columns.after"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} className="font-semibold" />,
        }),
      ]),
    [t, lang],
  );

  const rows = preview.data ?? [];
  const valueLabel = draft.change.startsWith("percent")
    ? t("products:bulkPrice.percent")
    : draft.change === "set"
      ? t("products:bulkPrice.price")
      : t("products:bulkPrice.amount");
  const error = preview.error ?? apply.error;
  return (
    <Dialog
      open
      size="xl"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("products:bulkPrice.title")}
      description={t("products:bulkPrice.previewHint")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={previewed === null || rows.length === 0 || preview.isFetching || apply.isPending}
            onClick={run}
          >
            {t("common:actions.apply")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 sm:grid-cols-3">
        {productIds.length > 0 ? (
          <p className="font-semibold sm:col-span-3">
            {t("products:bulkPrice.selected", { count: productIds.length })}
          </p>
        ) : (
          <>
            <Field id="bulk-category" label={t("products:fields.category")}>
              <NativeSelect
                id="bulk-category"
                value={draft.categoryId}
                onChange={(e) => update({ categoryId: e.target.value })}
              >
                <option value="">{t("products:bulkPrice.allCategories")}</option>
                {categories.map((c) => (
                  <option key={c.id} value={c.id}>
                    {localizedName(c, lang)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            <Field
              id="bulk-brand"
              label={t("products:extras.brand")}
              hint={t("products:bulkPrice.brandHint")}
            >
              <Input
                id="bulk-brand"
                dir="auto"
                value={draft.brand}
                onChange={(e) => update({ brand: e.target.value })}
              />
            </Field>
            {readsSuppliers ? (
              <Field id="bulk-supplier" label={t("products:extras.preferredSupplier")}>
                <NativeSelect
                  id="bulk-supplier"
                  value={draft.supplierId}
                  onChange={(e) => update({ supplierId: e.target.value })}
                >
                  <option value="">{t("products:bulkPrice.allSuppliers")}</option>
                  {suppliers.map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.name}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
            ) : null}
          </>
        )}
        <Field id="bulk-level" label={t("products:bulkPrice.level")}>
          <NativeSelect
            id="bulk-level"
            value={draft.priceLevelId}
            onChange={(e) => update({ priceLevelId: e.target.value })}
          >
            {levels.map((l) => (
              // The default level is the sale price itself (`priceLevelId: null`).
              <option key={l.id} value={l.isDefault ? "" : l.id}>
                {localizedName(l, lang)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id="bulk-change" label={t("products:bulkPrice.change")}>
          <NativeSelect
            id="bulk-change"
            value={draft.change}
            onChange={(e) => update({ change: e.target.value as ChangeChoice })}
          >
            {CHANGES.map((c) => (
              <option key={c} value={c}>
                {t(`products:bulkPrice.changes.${c}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field
          id="bulk-value"
          label={valueLabel}
          error={
            invalid
              ? t(draft.change.startsWith("percent") ? "validation:percentRange" : "validation:amountFormat")
              : undefined
          }
        >
          <NumericInput
            id="bulk-value"
            aria-invalid={invalid}
            value={draft.value}
            onValueChange={(value) => update({ value })}
          />
        </Field>
        <Field id="bulk-rounding" label={t("products:bulkPrice.rounding")}>
          <NativeSelect
            id="bulk-rounding"
            value={String(draft.roundTo)}
            onChange={(e) => update({ roundTo: Number(e.target.value) })}
          >
            {ROUNDINGS.map((r) => (
              <option key={r.key} value={r.value}>
                {t(`products:bulkPrice.roundings.${r.key}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
      </div>
      <div>
        <Button variant="secondary" onClick={showPreview}>
          {t("products:bulkPrice.preview")}
        </Button>
      </div>
      {previewed && preview.data ? (
        <section className="flex flex-col gap-2">
          <p role="status" className="font-semibold">
            {t("products:bulkPrice.count", { count: rows.length })}
          </p>
          <DataTable
            ariaLabel={t("products:bulkPrice.title")}
            data={rows}
            columns={columns}
            getRowId={(row) => row.productId}
            empty={t("products:bulkPrice.noProducts")}
          />
        </section>
      ) : null}
      {error ? (
        <p role="alert" className="text-danger">
          {errorText(error)}
        </p>
      ) : null}
    </Dialog>
  );
}
