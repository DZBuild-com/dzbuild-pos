import { zodResolver } from "@hookform/resolvers/zod";
import { Controller, useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { z } from "zod";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { useCan } from "@/features/stock/stock-parts";
import { formatAmountInput } from "@/lib/format";
import { useProductExtras, useSetProductExtras } from "@/lib/ipc/product-extras";
import { useSupplierSearch } from "@/lib/ipc/suppliers";
import type { Product, ProductExtras, ProductExtrasInput } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const schema = z.object({
  brand: z.string().max(80, "validation:tooLong"),
  supplierRef: z.string().max(48, "validation:tooLong"),
  altRefs: z.string().max(200, "validation:tooLong"),
  tags: z.string().max(200, "validation:tooLong"),
  minPrice: z.string().refine((v) => v.trim() === "" || parseAmount(v) !== null, "validation:amountFormat"),
  preferredSupplierId: z.string(),
  trackLots: z.boolean(),
  trackSerials: z.boolean(),
});

type Values = z.infer<typeof schema>;

const opt = (v: string) => (v.trim() === "" ? null : v.trim());

/**
 * The product's other details (PRD-002/061, PUR-003): brand, the supplier's reference, other
 * references and keywords the search finds, the floor price, the usual supplier and lot or serial
 * tracking. The floor price is a price: without `products.price` it is shown read-only (the
 * backend keeps the stored one). The variant criteria are set by the variants matrix only.
 */
export function ProductExtrasPanel({ product, editable }: { product: Product; editable: boolean }) {
  const extras = useProductExtras(product.id);
  const errorText = useErrorText();
  if (extras.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(extras.error)}
      </p>
    );
  }
  if (!extras.data) return null;
  return <ExtrasForm key={product.id} product={product} extras={extras.data} editable={editable} />;
}

function ExtrasForm({
  product,
  extras,
  editable,
}: {
  product: Product;
  extras: ProductExtras;
  editable: boolean;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const can = useCan();
  const readsSuppliers = can("suppliers.manage") || can("purchases.view");
  // Archived suppliers are listed only to keep the stored one a choice.
  const suppliers = (useSupplierSearch({ includeArchived: true }, readsSuppliers).data ?? []).filter(
    (s) => !s.archived || s.id === extras.preferredSupplierId,
  );
  const save = useSetProductExtras();
  const { register, control, handleSubmit, formState } = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: {
      brand: extras.brand ?? "",
      supplierRef: extras.supplierRef ?? "",
      altRefs: extras.altRefs ?? "",
      tags: extras.tags ?? "",
      minPrice: extras.minPrice === null ? "" : formatAmountInput(extras.minPrice, lang),
      preferredSupplierId: extras.preferredSupplierId ?? "",
      trackLots: extras.trackLots,
      trackSerials: extras.trackSerials,
    },
  });
  const err = (name: keyof Values) => {
    const message = formState.errors[name]?.message;
    return typeof message === "string" ? t(message) : undefined;
  };
  const submit = handleSubmit((v) => {
    const input: ProductExtrasInput = {
      brand: opt(v.brand),
      supplierRef: opt(v.supplierRef),
      altRefs: opt(v.altRefs),
      tags: opt(v.tags),
      attributes: extras.attributes,
      minPrice: v.minPrice.trim() === "" ? null : parseAmount(v.minPrice),
      trackLots: v.trackLots,
      trackSerials: v.trackSerials,
      preferredSupplierId: v.preferredSupplierId === "" ? null : v.preferredSupplierId,
    };
    save.mutate({ id: product.id, input }, { onSuccess: () => toast.success(t("products:extras.saved")) });
  });
  const optional = t("products:extras.optional");
  const attributes = Object.entries(extras.attributes);
  return (
    <form
      id="product-extras-form"
      noValidate
      onSubmit={(e) => void submit(e)}
      className="flex flex-col gap-4"
    >
      <fieldset disabled={!editable} className="grid gap-4 sm:grid-cols-2">
        <Field id="px-brand" label={t("products:extras.brand")} optional={optional} error={err("brand")}>
          <Input id="px-brand" dir="auto" {...register("brand")} />
        </Field>
        <Field
          id="px-supplierRef"
          label={t("products:extras.supplierRef")}
          optional={optional}
          error={err("supplierRef")}
        >
          <Input id="px-supplierRef" dir="ltr" {...register("supplierRef")} />
        </Field>
        <Field
          id="px-altRefs"
          label={t("products:extras.altRefs")}
          optional={optional}
          hint={t("products:extras.altRefsHint")}
          error={err("altRefs")}
        >
          <Input id="px-altRefs" dir="ltr" {...register("altRefs")} />
        </Field>
        <Field
          id="px-tags"
          label={t("products:extras.tags")}
          optional={optional}
          hint={t("products:extras.tagsHint")}
          error={err("tags")}
        >
          <Input id="px-tags" dir="auto" {...register("tags")} />
        </Field>
        <Field
          id="px-minPrice"
          label={t("products:extras.minPrice")}
          optional={optional}
          hint={t("products:extras.minPriceHint")}
          error={err("minPrice")}
        >
          <Controller
            name="minPrice"
            control={control}
            render={({ field }) => (
              <NumericInput
                id="px-minPrice"
                disabled={!can("products.price")}
                value={field.value}
                onValueChange={field.onChange}
              />
            )}
          />
        </Field>
        {readsSuppliers ? (
          <Field id="px-supplier" label={t("products:extras.preferredSupplier")} optional={optional}>
            {/* Controlled: the stored supplier is selected once the list arrives. */}
            <Controller
              name="preferredSupplierId"
              control={control}
              render={({ field }) => (
                <NativeSelect
                  id="px-supplier"
                  value={field.value}
                  onChange={(e) => field.onChange(e.target.value)}
                >
                  <option value="">{t("products:extras.noSupplier")}</option>
                  {suppliers.map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.name}
                    </option>
                  ))}
                </NativeSelect>
              )}
            />
          </Field>
        ) : null}
        {(["trackLots", "trackSerials"] as const).map((name) => (
          <Controller
            key={name}
            name={name}
            control={control}
            render={({ field }) => (
              <label className="flex items-center gap-3 font-semibold">
                <Checkbox checked={field.value} onCheckedChange={(v) => field.onChange(v === true)} />
                {t(`products:extras.${name}`)}
              </label>
            )}
          />
        ))}
      </fieldset>
      {attributes.length > 0 ? (
        <section className="flex flex-col gap-2">
          <h3 className="font-semibold">{t("products:extras.attributes")}</h3>
          <ul className="flex flex-wrap gap-2">
            {attributes.map(([name, value]) => (
              <li key={name} className="rounded-full bg-surface-2 px-3 py-1 text-sm">
                {t("products:extras.attribute", { name, value })}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {save.error ? (
        <p role="alert" className="text-danger">
          {errorText(save.error)}
        </p>
      ) : null}
      {editable ? (
        <div className="flex justify-end">
          <Button type="submit" variant="primary" disabled={save.isPending}>
            {t("products:extras.save")}
          </Button>
        </div>
      ) : null}
    </form>
  );
}
