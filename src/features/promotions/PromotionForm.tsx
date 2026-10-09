import { zodResolver } from "@hookform/resolvers/zod";
import { TriangleAlert, X } from "lucide-react";
import { useState } from "react";
import { type Resolver, Controller, useForm, useWatch } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { z } from "zod";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { ProductPicker } from "@/features/purchases/purchase-lines";
import { isDay } from "@/lib/dates";
import { formatAmountInput, formatPercentInput, parsePercentBp } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import { useCategories } from "@/lib/ipc/catalog";
import { errorKey, toAppError } from "@/lib/ipc/client";
import { useProduct } from "@/lib/ipc/products";
import { useSavePromotion } from "@/lib/ipc/promotions";
import { useSetting } from "@/lib/ipc/settings";
import type {
  Campaign,
  PromoKind,
  PromoScope,
  Promotion,
  PromotionInput,
  TriggerMode,
} from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty, parseAmount, parseQty, parseQtyOrZero } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const CAMPAIGNS = ["promotion", "solde", "liquidation", "deballage"] as const satisfies readonly Campaign[];
const KINDS = ["percent", "amount", "buy_x_get_y", "bundle_price"] as const satisfies readonly PromoKind[];
const TRIGGERS = ["auto", "manual"] as const satisfies readonly TriggerMode[];
const SCOPES = ["all", "categories", "products"] as const satisfies readonly PromoScope["kind"][];

const schema = z.object({
  nameFr: z.string().max(120, "validation:tooLong"),
  nameAr: z.string().max(120, "validation:tooLong"),
  nameEn: z.string().max(120, "validation:tooLong"),
  campaign: z.enum(CAMPAIGNS),
  kind: z.enum(KINDS),
  triggerMode: z.enum(TRIGGERS),
  scopeKind: z.enum(SCOPES),
  categoryIds: z.array(z.string()),
  productIds: z.array(z.string()),
  value: z.string(),
  buyQty: z.string(),
  getQty: z.string(),
  minQty: z.string().refine((v) => v.trim() === "" || parseQtyOrZero(v, 3) !== null, "validation:qtyFormat"),
  priority: z
    .string()
    .refine((v) => v === "" || (/^\d{1,3}$/.test(v) && Number(v) <= 100), "promotions:errors.priority"),
  startsOn: z.string(),
  endsOn: z.string(),
  active: z.boolean(),
});

type Values = z.infer<typeof schema>;
type Name = keyof Values;

/** Whole pieces typed ("2") → milli-units (2000), or null. */
function pieces(text: string): number | null {
  return parseQty(text, 0);
}

/** A named discount the cashier picks is an ordinary percentage promotion (PRM-003); the backend
 * applies every legal campaign automatically. */
function manualAllowed(kind: PromoKind, campaign: Campaign): boolean {
  return kind === "percent" && campaign === "promotion";
}

/** The checks one field alone cannot make, as translation keys per field. */
function crossErrors(v: Values): Partial<Record<Name, string>> {
  const errors: Partial<Record<Name, string>> = {};
  if (![v.nameAr, v.nameFr, v.nameEn].some((n) => n.trim() !== "")) errors.nameFr = "validation:nameRequired";
  if (v.kind === "percent") {
    const bp = parsePercentBp(v.value);
    if (bp === null || bp < 1) errors.value = "promotions:errors.percent";
  } else if (v.kind === "amount" || v.kind === "bundle_price") {
    const amount = parseAmount(v.value);
    if (amount === null || amount <= 0) errors.value = "promotions:errors.amount";
  }
  if (v.kind === "buy_x_get_y") {
    if (pieces(v.buyQty) === null) errors.buyQty = "promotions:errors.pieces";
    if (pieces(v.getQty) === null) errors.getQty = "promotions:errors.pieces";
  }
  if (v.kind === "bundle_price" && (pieces(v.buyQty) ?? 0) < 2_000)
    errors.buyQty = "promotions:errors.bundle";
  for (const name of ["startsOn", "endsOn"] as const) {
    if (v[name] !== "" && !isDay(v[name])) errors[name] = "errors:date_invalid";
    else if (v[name] === "" && v.campaign !== "promotion") errors[name] = "errors:promotion_dates_required";
  }
  if (!errors.startsOn && !errors.endsOn && v.startsOn !== "" && v.endsOn !== "" && v.startsOn > v.endsOn) {
    errors.endsOn = "validation:rangeOrder";
  }
  if (v.scopeKind === "categories" && v.categoryIds.length === 0)
    errors.categoryIds = "promotions:errors.categories";
  if (v.scopeKind === "products" && v.productIds.length === 0)
    errors.productIds = "promotions:errors.products";
  return errors;
}

const schemaResolver = zodResolver(schema);

/** The schema's field checks plus {@link crossErrors}: every problem shows at once. */
const resolver: Resolver<Values> = async (values, context, options) => {
  const result = await schemaResolver(values, context, options);
  const extra = Object.entries(crossErrors(values)).filter(([name]) => !(name in result.errors));
  if (extra.length === 0) return result;
  const errors = { ...result.errors };
  for (const [name, message] of extra) Object.assign(errors, { [name]: { type: "validate", message } });
  return { values: {}, errors };
};

function toValues(p: Promotion | null, lang: Lang): Values {
  const scope = p?.scope ?? { kind: "all" };
  const kind = p?.kind ?? "percent";
  return {
    nameFr: p?.nameFr ?? "",
    nameAr: p?.nameAr ?? "",
    nameEn: p?.nameEn ?? "",
    campaign: p?.campaign ?? "promotion",
    kind,
    triggerMode: p?.triggerMode ?? "auto",
    scopeKind: scope.kind,
    categoryIds: scope.kind === "categories" ? scope.ids : [],
    productIds: scope.kind === "products" ? scope.ids : [],
    value: !p
      ? ""
      : kind === "percent"
        ? formatPercentInput(p.value, lang)
        : kind === "buy_x_get_y"
          ? ""
          : formatAmountInput(p.value, lang),
    buyQty: p?.buyQty ? formatQty(p.buyQty, lang) : "",
    getQty: p?.getQty ? formatQty(p.getQty, lang) : "",
    minQty: p && p.minQty !== 0 ? formatQty(p.minQty, lang) : "",
    priority: p && p.priority !== 0 ? String(p.priority) : "",
    startsOn: p?.startsOn ?? "",
    endsOn: p?.endsOn ?? "",
    active: p?.active ?? true,
  };
}

function toInput(v: Values): PromotionInput {
  const scope: PromoScope =
    v.scopeKind === "categories"
      ? { kind: "categories", ids: v.categoryIds }
      : v.scopeKind === "products"
        ? { kind: "products", ids: v.productIds }
        : { kind: "all" };
  const value =
    v.kind === "percent"
      ? (parsePercentBp(v.value) ?? 0)
      : v.kind === "buy_x_get_y"
        ? 0
        : (parseAmount(v.value) ?? 0);
  return {
    nameAr: v.nameAr.trim(),
    nameFr: v.nameFr.trim(),
    nameEn: v.nameEn.trim(),
    kind: v.kind,
    campaign: v.campaign,
    triggerMode: manualAllowed(v.kind, v.campaign) ? v.triggerMode : "auto",
    scope,
    value,
    buyQty: v.kind === "buy_x_get_y" || v.kind === "bundle_price" ? pieces(v.buyQty) : null,
    getQty: v.kind === "buy_x_get_y" ? pieces(v.getQty) : null,
    minQty: v.minQty.trim() === "" ? 0 : (parseQtyOrZero(v.minQty, 3) ?? 0),
    priority: v.priority === "" ? 0 : Number(v.priority),
    startsOn: v.startsOn === "" ? null : v.startsOn,
    endsOn: v.endsOn === "" ? null : v.endsOn,
    active: v.active,
  };
}

/** The backend's field names (`AppError.field`) → the form field that shows the error. */
function serverField(field: string, scopeKind: Values["scopeKind"]): Name | null {
  const direct: Partial<Record<string, Name>> = {
    name: "nameFr",
    nameAr: "nameAr",
    nameFr: "nameFr",
    nameEn: "nameEn",
    value: "value",
    buyQty: "buyQty",
    getQty: "getQty",
    triggerMode: "triggerMode",
    priority: "priority",
    minQty: "minQty",
    startsOn: "startsOn",
    endsOn: "endsOn",
  };
  if (field === "scope") return scopeKind === "products" ? "productIds" : "categoryIds";
  return direct[field] ?? null;
}

/** The wali's soldes windows as stored (`promotions.soldes_windows`). */
function windowsOf(value: unknown): { from: string; to: string }[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((w: unknown) => {
    const { from, to } = (w ?? {}) as { from?: unknown; to?: unknown };
    return typeof from === "string" && typeof to === "string" ? [{ from, to }] : [];
  });
}

/**
 * PRM-001…006, 021: creates or edits a promotion — percentage, amount per item, "X bought, Y free"
 * or a bundle price; on everything, some categories (with their sub-categories) or some products;
 * applied automatically at the till or picked by the cashier as a named discount; with its
 * priority, dates and minimum quantity. A campaign of soldes outside the wali's windows is flagged
 * here as a hint only: the backend's `outsideSoldesWindow` is the record the list shows.
 */
export function PromotionForm({
  promotion = null,
  readOnly = false,
  onSaved,
  onCancel,
}: {
  promotion?: Promotion | null;
  readOnly?: boolean;
  onSaved: (promotion: Promotion) => void;
  onCancel?: () => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const save = useSavePromotion();
  const windows = windowsOf(useSetting("promotions.soldes_windows").data);
  const [failure, setFailure] = useState<unknown>(null);
  const { register, control, handleSubmit, formState, setError } = useForm<Values>({
    resolver,
    defaultValues: toValues(promotion, lang),
  });
  const [kind, campaign, scopeKind, startsOn, endsOn] = useWatch({
    control,
    name: ["kind", "campaign", "scopeKind", "startsOn", "endsOn"],
  });
  const err = (name: Name) => {
    const message = formState.errors[name]?.message;
    return typeof message === "string" ? t(message) : undefined;
  };
  const outside =
    campaign === "solde" &&
    isDay(startsOn) &&
    isDay(endsOn) &&
    startsOn <= endsOn &&
    !windows.some((w) => w.from <= startsOn && endsOn <= w.to);
  const mayBeManual = manualAllowed(kind, campaign);

  const submit = handleSubmit((values) => {
    setFailure(null);
    save.mutate(
      { id: promotion?.id ?? null, input: toInput(values) },
      {
        onSuccess: onSaved,
        onError: (error) => {
          const field = serverField(toAppError(error).field ?? "", values.scopeKind);
          if (field) {
            setError(
              field,
              { type: "server", message: errorKey(error, (k) => i18n.exists(k)) },
              { shouldFocus: true },
            );
          } else {
            setFailure(error);
          }
        },
      },
    );
  });

  const optional = t("promotions:optional");
  const amountField = (label: string) => (
    <Field id="promo-value" label={label} error={err("value")}>
      <Controller
        name="value"
        control={control}
        render={({ field }) => (
          <NumericInput
            id="promo-value"
            aria-invalid={Boolean(err("value"))}
            value={field.value}
            onValueChange={field.onChange}
          />
        )}
      />
    </Field>
  );
  const piecesField = (name: "buyQty" | "getQty", label: string) => (
    <Field id={`promo-${name}`} label={label} error={err(name)}>
      <Controller
        name={name}
        control={control}
        render={({ field }) => (
          <NumericInput
            id={`promo-${name}`}
            kind="integer"
            aria-invalid={Boolean(err(name))}
            value={field.value}
            onValueChange={field.onChange}
          />
        )}
      />
    </Field>
  );

  return (
    <form noValidate onSubmit={(e) => void submit(e)} className="flex flex-col gap-4">
      <fieldset disabled={readOnly} className="grid gap-4 sm:grid-cols-3">
        <Field id="promo-nameFr" label={t("promotions:fields.nameFr")} error={err("nameFr")}>
          <Input id="promo-nameFr" lang="fr" aria-invalid={Boolean(err("nameFr"))} {...register("nameFr")} />
        </Field>
        <Field id="promo-nameAr" label={t("promotions:fields.nameAr")} error={err("nameAr")}>
          <Input id="promo-nameAr" dir="rtl" lang="ar" {...register("nameAr")} />
        </Field>
        <Field id="promo-nameEn" label={t("promotions:fields.nameEn")} error={err("nameEn")}>
          <Input id="promo-nameEn" lang="en" {...register("nameEn")} />
        </Field>
        <div className="sm:col-span-2">
          <Field
            id="promo-campaign"
            label={t("promotions:fields.campaign")}
            hint={t("promotions:hints.campaign")}
          >
            <NativeSelect id="promo-campaign" {...register("campaign")}>
              {CAMPAIGNS.map((c) => (
                <option key={c} value={c}>
                  {t(`promotions:campaigns.${c}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
        </div>
        <Field id="promo-kind" label={t("promotions:fields.kind")}>
          <NativeSelect id="promo-kind" {...register("kind")}>
            {KINDS.map((k) => (
              <option key={k} value={k}>
                {t(`promotions:kinds.${k}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        {kind === "percent" ? amountField(t("promotions:fields.percent")) : null}
        {kind === "amount" ? amountField(t("promotions:fields.amount")) : null}
        {kind === "buy_x_get_y" ? (
          <>
            {piecesField("buyQty", t("promotions:fields.buyQty"))}
            {piecesField("getQty", t("promotions:fields.getQty"))}
          </>
        ) : null}
        {kind === "bundle_price" ? (
          <>
            {piecesField("buyQty", t("promotions:fields.bundleQty"))}
            {amountField(t("promotions:fields.bundlePrice"))}
          </>
        ) : null}
        <Field
          id="promo-trigger"
          label={t("promotions:fields.trigger")}
          hint={t("promotions:hints.trigger")}
          error={err("triggerMode")}
        >
          <Controller
            name="triggerMode"
            control={control}
            render={({ field }) => (
              <NativeSelect
                id="promo-trigger"
                value={mayBeManual ? field.value : "auto"}
                onChange={(e) => field.onChange(e.target.value)}
              >
                {TRIGGERS.map((m) => (
                  <option key={m} value={m} disabled={m === "manual" && !mayBeManual}>
                    {t(`promotions:triggers.${m}`)}
                  </option>
                ))}
              </NativeSelect>
            )}
          />
        </Field>
        <Field id="promo-scope" label={t("promotions:fields.scope")}>
          <NativeSelect id="promo-scope" {...register("scopeKind")}>
            {SCOPES.map((s) => (
              <option key={s} value={s}>
                {t(`promotions:scopes.${s}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id="promo-startsOn" label={t("promotions:fields.startsOn")} error={err("startsOn")}>
          <Input
            id="promo-startsOn"
            type="date"
            dir="ltr"
            aria-invalid={Boolean(err("startsOn"))}
            {...register("startsOn")}
          />
        </Field>
        <Field id="promo-endsOn" label={t("promotions:fields.endsOn")} error={err("endsOn")}>
          <Input
            id="promo-endsOn"
            type="date"
            dir="ltr"
            aria-invalid={Boolean(err("endsOn"))}
            {...register("endsOn")}
          />
        </Field>
        <Field
          id="promo-minQty"
          label={t("promotions:fields.minQty")}
          optional={optional}
          hint={t("promotions:hints.minQty")}
          error={err("minQty")}
        >
          <Controller
            name="minQty"
            control={control}
            render={({ field }) => (
              <NumericInput id="promo-minQty" kind="qty" value={field.value} onValueChange={field.onChange} />
            )}
          />
        </Field>
        <Field
          id="promo-priority"
          label={t("promotions:fields.priority")}
          optional={optional}
          hint={t("promotions:hints.priority")}
          error={err("priority")}
        >
          <Controller
            name="priority"
            control={control}
            render={({ field }) => (
              <NumericInput
                id="promo-priority"
                kind="integer"
                value={field.value}
                onValueChange={field.onChange}
              />
            )}
          />
        </Field>
        <Controller
          name="active"
          control={control}
          render={({ field }) => (
            <label className="flex items-center gap-3 self-end pb-2 font-semibold">
              <Checkbox checked={field.value} onCheckedChange={(v) => field.onChange(v === true)} />
              {t("promotions:fields.active")}
            </label>
          )}
        />
        {scopeKind === "categories" ? (
          <div className="sm:col-span-3">
            <Controller
              name="categoryIds"
              control={control}
              render={({ field }) => (
                <CategoryChoice value={field.value} onChange={field.onChange} error={err("categoryIds")} />
              )}
            />
          </div>
        ) : null}
        {scopeKind === "products" ? (
          <div className="sm:col-span-3">
            <Controller
              name="productIds"
              control={control}
              render={({ field }) => (
                <ProductChoice value={field.value} onChange={field.onChange} error={err("productIds")} />
              )}
            />
          </div>
        ) : null}
      </fieldset>
      {outside ? (
        <p
          role="alert"
          className="flex items-start gap-2 rounded-card bg-warn-soft p-3 font-semibold text-warn"
        >
          <TriangleAlert aria-hidden className="mt-0.5 size-5 shrink-0" />
          {t("promotions:outsideWindow")}
        </p>
      ) : null}
      {failure ? (
        <p role="alert" className="text-danger">
          {errorText(failure)}
        </p>
      ) : null}
      {readOnly ? (
        <p className="text-ink-2">{t("promotions:readOnly")}</p>
      ) : (
        <div className="flex flex-wrap justify-end gap-2">
          {onCancel ? (
            <Button variant="outline" onClick={onCancel}>
              {t("common:actions.cancel")}
            </Button>
          ) : null}
          <Button type="submit" variant="primary" disabled={save.isPending}>
            {t("common:actions.save")}
          </Button>
        </div>
      )}
    </form>
  );
}

/** The categories a promotion covers (their sub-categories too, in the backend). */
function CategoryChoice({
  value,
  onChange,
  error,
}: {
  value: string[];
  onChange: (ids: string[]) => void;
  error?: string;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const categories = (useCategories().data ?? []).filter((c) => !c.archived || value.includes(c.id));
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="mb-1 font-semibold">{t("promotions:fields.categories")}</legend>
      <div className="flex flex-wrap gap-x-6 gap-y-2">
        {categories.map((c) => (
          <label key={c.id} className="flex min-h-11 items-center gap-3">
            <Checkbox
              checked={value.includes(c.id)}
              onCheckedChange={(checked) =>
                onChange(checked === true ? [...value, c.id] : value.filter((id) => id !== c.id))
              }
            />
            {localizedName(c, lang)}
          </label>
        ))}
      </div>
      {error ? (
        <p role="alert" className="text-sm text-danger">
          {error}
        </p>
      ) : null}
    </fieldset>
  );
}

/** The products a promotion covers, added by scan or search. */
function ProductChoice({
  value,
  onChange,
  error,
}: {
  value: string[];
  onChange: (ids: string[]) => void;
  error?: string;
}) {
  return (
    <div className="flex flex-col gap-2">
      <ProductPicker
        onPick={(p) => {
          if (!value.includes(p.productId)) onChange([...value, p.productId]);
        }}
      />
      {value.length > 0 ? (
        <ul className="flex flex-wrap gap-2">
          {value.map((id) => (
            <ProductChip key={id} id={id} onRemove={() => onChange(value.filter((x) => x !== id))} />
          ))}
        </ul>
      ) : null}
      {error ? (
        <p role="alert" className="text-sm text-danger">
          {error}
        </p>
      ) : null}
    </div>
  );
}

function ProductChip({ id, onRemove }: { id: string; onRemove: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const product = useProduct(id).data;
  const name = product ? localizedName(product, lang) : "";
  return (
    <li className="flex items-center gap-1 rounded-full bg-surface-2 ps-3">
      <span>{name}</span>
      <Button
        variant="ghost"
        size="icon"
        aria-label={t("promotions:removeProduct", { name })}
        title={t("promotions:removeProduct", { name })}
        onClick={onRemove}
      >
        <X aria-hidden />
      </Button>
    </li>
  );
}
