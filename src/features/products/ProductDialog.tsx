import { History } from "lucide-react";
import { Suspense, useState } from "react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { ScanInput } from "@/components/scan-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { JewelTab } from "@/features/jewellery/JewelTab";
import { MedicineTab, VariantGrid } from "@/features/packs/pack-screens";
import { EntityHistoryDialog } from "@/features/settings/EntityHistoryDialog";
import { ProductWholesale } from "@/features/wholesale/ProductWholesale";
import { formatAmountInput } from "@/lib/format";
import { useCategories, usePriceLevels, useTaxRates, useUnits } from "@/lib/ipc/catalog";
import { toAppError } from "@/lib/ipc/client";
import { useScaleItems, useSetScaleItem } from "@/lib/ipc/labels";
import { useTillLink } from "@/lib/ipc/lan";
import { usePackState } from "@/lib/ipc/pack-state";
import {
  useArchiveProduct,
  useCreateProduct,
  useDuplicateProduct,
  useRestoreProduct,
  useUpdateProduct,
} from "@/lib/ipc/products";
import type { Category, PriceLevel, Product, Session, TaxRate, Unit } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import {
  EDITABLE_KINDS,
  type ProductForm,
  canEditKind,
  emptyForm,
  productResolver,
  toForm,
  toInput,
} from "./product-form";
import { PriceBreaksEditor } from "./PriceBreaksEditor";
import { PriceHelper } from "./PriceHelper";
import { ProductExtrasPanel } from "./ProductExtrasPanel";
import { ProductPhoto } from "./ProductPhoto";
import { SCALE_FIELDS, ScaleFields, type ScaleDraft, scaleDraft, scaleInput } from "./ScaleFields";
import { StockPanel } from "./StockPanel";
import { VariantsDialog, VariantsPanel } from "./VariantsDialog";

type Tab = "product" | "stock" | "extras" | "breaks" | "variants" | "jewel" | "medicine" | "grid";

/** Loads the lists the form needs first, so its default unit and VAT rate are real ones. */
export function ProductDialog({
  product,
  session,
  onClose,
}: {
  product: Product | null;
  session: Session;
  onClose: () => void;
}) {
  const units = useUnits();
  const rates = useTaxRates();
  const categories = useCategories();
  const levels = usePriceLevels();
  if (!units.data || !rates.data || !categories.data || !levels.data) return null;
  return (
    <ProductEditor
      product={product}
      session={session}
      onClose={onClose}
      units={units.data.filter((u) => !u.archived)}
      rates={rates.data.filter((r) => r.active)}
      categories={categories.data.filter((c) => !c.archived)}
      levels={levels.data.filter((l) => !l.isDefault && !l.archived)}
    />
  );
}

function ProductEditor({
  product,
  session,
  onClose,
  units,
  rates,
  categories,
  levels,
}: {
  product: Product | null;
  session: Session;
  onClose: () => void;
  units: Unit[];
  rates: TaxRate[];
  categories: Category[];
  levels: PriceLevel[];
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const create = useCreateProduct();
  const update = useUpdateProduct();
  const archive = useArchiveProduct();
  const restore = useRestoreProduct();
  const duplicate = useDuplicateProduct();
  const setScale = useSetScaleItem();
  // the scale items are the main PC's: read-only on a till
  const till = useTillLink();
  const [tab, setTab] = useState<Tab>("product");
  // the scale settings as typed; `null` until one is changed (the stored ones are shown)
  const [scale, setScaleEdit] = useState<ScaleDraft | null>(null);
  const [history, setHistory] = useState(false);
  const [matrix, setMatrix] = useState(false);
  const can = (p: Session["permissions"][number]) => session.permissions.includes(p);
  const seesCosts = can("reports.profit");
  const editable = can("products.edit");
  // A new product's first prices need `products.edit` only; changing an existing product's prices
  // needs `products.price` (the backend keeps the stored ones otherwise, spec §4.5).
  const pricesLocked = product !== null && !can("products.price");
  const defaultRate = rates.find((r) => r.rateBp === 1_900)?.id ?? rates[0]?.id ?? "";
  const { register, control, handleSubmit, formState, setValue } = useForm<ProductForm>({
    resolver: productResolver,
    defaultValues: product ? toForm(product, lang) : emptyForm(units[0]?.id ?? "", defaultRate),
  });
  const [typedPrice, typedCost, taxRateId, typedPlu] = useWatch({
    control,
    name: ["sellPrice", "costPrice", "taxRateId", "plu"],
  });
  // PRD-074: only a product with a PLU has scale settings
  const scaleItems = useScaleItems(product !== null && product.plu !== null);
  const scaleValue = scale ?? scaleDraft(scaleItems.data?.find((item) => item.productId === product?.id));
  // The margin helper needs a cost: the typed one, else the stored CMUP.
  const helperCost = typedCost.trim() === "" ? (product?.costPrice ?? null) : parseAmount(typedCost);
  const err = (name: keyof ProductForm) => {
    const message = formState.errors[name]?.message;
    return typeof message === "string" ? t(message) : undefined;
  };
  const levelErr = (levelId: string) => {
    const message = formState.errors.levelPrices?.[levelId]?.message;
    return typeof message === "string" ? t(message) : undefined;
  };
  const submit = handleSubmit((values) => {
    const input = toInput(values, product);
    if (product) {
      update.mutate(
        { id: product.id, input },
        {
          // the scale settings need the PLU this save stored
          onSuccess: () => {
            if (scale !== null && input.plu !== null) {
              setScale.mutate({ productId: product.id, input: scaleInput(scale) }, { onSuccess: onClose });
            } else onClose();
          },
        },
      );
    } else create.mutate(input, { onSuccess: onClose });
  });
  const title = product ? localizedName(product, lang) : t("products:dialog.new");
  // M8: the medicine's tab while the pharmacy module is on, a variant model's stock grid while the
  // variant grid is, and a piece of jewellery (VRT-140: stocked one by one, a simple product or a
  // variant) while the jewellery module is
  const modules = usePackState().data?.modules ?? [];
  const jewellery = modules.includes("jewellery");
  const tabs: Tab[] =
    product === null
      ? []
      : [
          "product",
          "stock",
          "extras",
          "breaks",
          ...(product.kind === "variant_parent" ? ["variants" as const] : []),
          ...(jewellery && (product.kind === "simple" || product.kind === "variant")
            ? ["jewel" as const]
            : []),
          ...(modules.includes("pharmacy") ? ["medicine" as const] : []),
          ...(product.kind === "variant_parent" && modules.includes("variant_grid") ? ["grid" as const] : []),
        ];
  const tabLabel = (k: Tab) =>
    k === "jewel"
      ? t("jewellery:tab.title")
      : k === "medicine"
        ? t("pharmacy:tab")
        : k === "grid"
          ? t("products:variantGrid.tab")
          : t(`products:dialog.tabs.${k}`);
  // a refusal naming a scale field shows under that field
  const scaleField = SCALE_FIELDS.some((f) => f === toAppError(setScale.error).field);
  const error =
    create.error ??
    update.error ??
    archive.error ??
    restore.error ??
    duplicate.error ??
    (scaleField ? null : setScale.error);
  return (
    <>
      <Dialog
        open={!history && !matrix}
        size="lg"
        onOpenChange={(open) => (open ? undefined : onClose())}
        title={title}
        footer={
          editable && tab === "product" ? (
            <>
              {product && can("audit.view") ? (
                <Button variant="ghost" onClick={() => setHistory(true)}>
                  <History aria-hidden />
                  {t("products:dialog.history")}
                </Button>
              ) : null}
              {product ? (
                product.archived ? (
                  <Button
                    variant="outline"
                    onClick={() => restore.mutate(product.id, { onSuccess: onClose })}
                  >
                    {t("products:dialog.restore")}
                  </Button>
                ) : (
                  <>
                    <Button
                      variant="outline"
                      onClick={() => duplicate.mutate(product.id, { onSuccess: onClose })}
                    >
                      {t("products:dialog.duplicate")}
                    </Button>
                    <Button
                      variant="danger"
                      onClick={() => archive.mutate(product.id, { onSuccess: onClose })}
                    >
                      {t("products:dialog.archive")}
                    </Button>
                  </>
                )
              ) : null}
              <Button
                type="submit"
                form="product-form"
                variant="primary"
                disabled={create.isPending || update.isPending || setScale.isPending}
              >
                {t("products:dialog.save")}
              </Button>
            </>
          ) : undefined
        }
      >
        {product ? (
          <div role="tablist" className="flex flex-wrap gap-2">
            {tabs.map((k) => (
              <button
                key={k}
                type="button"
                role="tab"
                aria-selected={tab === k}
                onClick={() => setTab(k)}
                className={cn(
                  "min-h-11 rounded-control px-4 font-semibold",
                  tab === k ? "bg-brand-soft" : "text-ink-2",
                )}
              >
                {tabLabel(k)}
              </button>
            ))}
          </div>
        ) : null}
        {product && tab !== "product" ? (
          <OtherTab
            tab={tab}
            product={product}
            session={session}
            editable={editable}
            onMatrix={() => setMatrix(true)}
          />
        ) : (
          <form
            id="product-form"
            noValidate
            onSubmit={(e) => void submit(e)}
            className="grid gap-4 sm:grid-cols-2"
          >
            <fieldset disabled={!editable} className="contents">
              <Field id="p-nameFr" label={t("products:fields.nameFr")} error={err("nameFr")}>
                <Input id="p-nameFr" lang="fr" {...register("nameFr")} />
              </Field>
              <Field id="p-nameAr" label={t("products:fields.nameAr")} error={err("nameAr")}>
                <Input id="p-nameAr" dir="rtl" lang="ar" {...register("nameAr")} />
              </Field>
              <Field id="p-nameEn" label={t("products:fields.nameEn")} error={err("nameEn")}>
                <Input id="p-nameEn" lang="en" {...register("nameEn")} />
              </Field>
              <Field id="p-reference" label={t("products:fields.reference")} error={err("reference")}>
                <Input id="p-reference" dir="ltr" {...register("reference")} />
              </Field>
              <Field id="p-barcode" label={t("products:fields.barcode")} error={err("barcode")}>
                <ScanInput
                  id="p-barcode"
                  dir="ltr"
                  {...register("barcode")}
                  onScan={(code) => setValue("barcode", code, { shouldDirty: true, shouldValidate: true })}
                />
              </Field>
              <Field id="p-category" label={t("products:fields.category")}>
                <NativeSelect id="p-category" {...register("categoryId")}>
                  <option value="">{t("products:fields.noCategory")}</option>
                  {categories.map((c) => (
                    <option key={c.id} value={c.id}>
                      {localizedName(c, lang)}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
              <Field id="p-unit" label={t("products:fields.unit")} error={err("unitId")}>
                <NativeSelect id="p-unit" {...register("unitId")}>
                  {units.map((u) => (
                    <option key={u.id} value={u.id}>
                      {localizedName(u, lang)}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
              <Field id="p-rate" label={t("products:fields.taxRate")} error={err("taxRateId")}>
                <NativeSelect id="p-rate" {...register("taxRateId")}>
                  {rates.map((r) => (
                    <option key={r.id} value={r.id}>
                      {localizedName(r, lang)}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
              <Field id="p-price" label={t("products:fields.sellPrice")} error={err("sellPrice")}>
                <Controller
                  name="sellPrice"
                  control={control}
                  render={({ field }) => (
                    <NumericInput
                      id="p-price"
                      disabled={pricesLocked}
                      value={field.value}
                      onValueChange={field.onChange}
                    />
                  )}
                />
              </Field>
              {seesCosts ? (
                <Field id="p-cost" label={t("products:fields.costPrice")} error={err("costPrice")}>
                  <Controller
                    name="costPrice"
                    control={control}
                    render={({ field }) => (
                      <NumericInput id="p-cost" value={field.value} onValueChange={field.onChange} />
                    )}
                  />
                </Field>
              ) : null}
              {seesCosts && helperCost !== null && helperCost > 0 ? (
                <div className="sm:col-span-2">
                  <PriceHelper
                    cost={helperCost}
                    price={parseAmount(typedPrice)}
                    taxRateId={taxRateId}
                    onUsePrice={
                      pricesLocked
                        ? undefined
                        : (price) =>
                            setValue("sellPrice", formatAmountInput(price, lang), {
                              shouldDirty: true,
                              shouldValidate: true,
                            })
                    }
                  />
                </div>
              ) : null}
              {levels.map((level) => (
                <Field
                  key={level.id}
                  id={`p-level-${level.id}`}
                  label={t("products:fields.levelPrice", { level: localizedName(level, lang) })}
                  optional={t("setup:shop.optional")}
                  error={levelErr(level.id)}
                >
                  <Controller
                    name={`levelPrices.${level.id}`}
                    control={control}
                    // A level without a price of its own starts empty (not undefined): the
                    // schema reads every level field as text, and an empty one sells at the
                    // default price.
                    defaultValue=""
                    render={({ field }) => (
                      <NumericInput
                        id={`p-level-${level.id}`}
                        disabled={pricesLocked}
                        value={field.value}
                        onValueChange={field.onChange}
                      />
                    )}
                  />
                </Field>
              ))}
              <Field id="p-min" label={t("products:fields.minStock")} error={err("minStock")}>
                <Controller
                  name="minStock"
                  control={control}
                  render={({ field }) => (
                    <NumericInput id="p-min" kind="qty" value={field.value} onValueChange={field.onChange} />
                  )}
                />
              </Field>
              <Field
                id="p-plu"
                label={t("products:fields.plu")}
                error={err("plu")}
                optional={t("setup:shop.optional")}
              >
                <Controller
                  name="plu"
                  control={control}
                  render={({ field }) => (
                    <NumericInput
                      id="p-plu"
                      kind="integer"
                      value={field.value}
                      onValueChange={field.onChange}
                    />
                  )}
                />
              </Field>
              {typedPlu.trim() === "" ? null : product ? (
                <ScaleFields
                  value={scaleValue}
                  onChange={setScaleEdit}
                  error={setScale.error}
                  readOnly={till.isTill}
                />
              ) : (
                <p className="text-sm text-ink-2 sm:col-span-2">{t("products:scale.afterSave")}</p>
              )}
              <Field id="p-kind" label={t("products:fields.kind")}>
                {canEditKind(product) ? (
                  <NativeSelect id="p-kind" {...register("kind")}>
                    {EDITABLE_KINDS.map((kind) => (
                      <option key={kind} value={kind}>
                        {t(`products:kinds.${kind}`)}
                      </option>
                    ))}
                  </NativeSelect>
                ) : (
                  <Input id="p-kind" readOnly value={t(`products:kinds.${product?.kind ?? "simple"}`)} />
                )}
              </Field>
              {(["trackStock", "allowNegativeStock", "isWeighed", "availableAtTill"] as const).map((name) => (
                <Controller
                  key={name}
                  name={name}
                  control={control}
                  render={({ field }) => (
                    <label className="flex items-center gap-3 font-semibold">
                      <Checkbox checked={field.value} onCheckedChange={(v) => field.onChange(v === true)} />
                      {t(`products:fields.${name}`)}
                    </label>
                  )}
                />
              ))}
            </fieldset>
          </form>
        )}
        {/* M8 wholesale: the minimum order and "Créer un pack", while the module is on */}
        {product && tab === "product" ? <ProductWholesale product={product} session={session} /> : null}
        {error ? (
          <p role="alert" className="text-danger">
            {errorText(error)}
          </p>
        ) : null}
      </Dialog>
      {matrix && product ? <VariantsDialog product={product} onClose={() => setMatrix(false)} /> : null}
      {history && product ? (
        <EntityHistoryDialog
          entityType="product"
          entityId={product.id}
          title={title}
          onClose={() => setHistory(false)}
        />
      ) : null}
    </>
  );
}

/** The tabs of an existing product besides its form. */
function OtherTab({
  tab,
  product,
  session,
  editable,
  onMatrix,
}: {
  tab: Exclude<Tab, "product">;
  product: Product;
  session: Session;
  editable: boolean;
  onMatrix: () => void;
}) {
  switch (tab) {
    case "stock":
      return <StockPanel product={product} session={session} />;
    case "extras":
      return (
        <div className="flex flex-col gap-6">
          <ProductPhoto product={product} editable={editable} />
          <ProductExtrasPanel product={product} editable={editable} />
        </div>
      );
    case "breaks":
      return <PriceBreaksEditor product={product} />;
    case "variants":
      return <VariantsPanel product={product} editable={editable} onGenerate={onMatrix} />;
    case "jewel":
      return <JewelTab product={product} editable={editable} />;
    case "medicine":
      return (
        <Suspense fallback={null}>
          <MedicineTab product={product} editable={editable} />
        </Suspense>
      );
    case "grid":
      return (
        <Suspense fallback={null}>
          <VariantGrid parentId={product.id} />
        </Suspense>
      );
  }
}
