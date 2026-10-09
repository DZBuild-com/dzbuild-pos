import { zodResolver } from "@hookform/resolvers/zod";
import { useState } from "react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { z } from "zod";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { toAsciiDigits } from "@/lib/digits";
import { formatAmountInput, formatPercentInput, parsePercentBp } from "@/lib/format";
import { LANGS, isLang } from "@/lib/i18n";
import { useDatapack } from "@/lib/ipc/app";
import { useSession } from "@/lib/ipc/auth";
import { usePriceLevels } from "@/lib/ipc/catalog";
import { errorKey, toAppError } from "@/lib/ipc/client";
import { useCreateCustomer, useUpdateCustomer } from "@/lib/ipc/customers";
import type { Customer, CustomerInput, CustomerType } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const CUSTOMER_TYPES = [
  "individual",
  "reseller",
  "garage",
  "contractor",
  "restaurant",
  "company",
  "other",
] as const satisfies readonly CustomerType[];

const schema = z.object({
  name: z
    .string()
    .refine((v) => v.trim() !== "", "validation:required")
    .refine((v) => v.trim().length <= 120, "validation:tooLong"),
  customerType: z.enum(CUSTOMER_TYPES),
  phone: z.string(),
  phone2: z.string(),
  email: z.string(),
  address: z.string(),
  wilaya: z.string(),
  nif: z.string(),
  nis: z.string(),
  rc: z.string(),
  ai: z.string(),
  nin: z.string(),
  priceLevelId: z.string(),
  discount: z
    .string()
    .refine((v) => v.trim() === "" || parsePercentBp(v) !== null, "validation:percentRange"),
  creditLimit: z
    .string()
    .refine((v) => v.trim() === "" || parseAmount(v) !== null, "validation:amountFormat"),
  lang: z.string(),
  notes: z.string(),
  isFavourite: z.boolean(),
  marketingConsent: z.boolean(),
});

type Values = z.infer<typeof schema>;
type Name = keyof Values;

/** The backend's field names (`AppError.field`) → the form field that shows the error. */
const SERVER_FIELDS: Partial<Record<string, Name>> = {
  name: "name",
  phone: "phone",
  phone2: "phone2",
  email: "email",
  address: "address",
  wilaya: "wilaya",
  nif: "nif",
  nis: "nis",
  rc: "rc",
  ai: "ai",
  nin: "nin",
  priceLevelId: "priceLevelId",
  discountBp: "discount",
  creditLimit: "creditLimit",
  notes: "notes",
};

/** A reseller's invoices carry these (CRM-006, CIDTA 183 ter): the backend requires them. */
const RESELLER_REQUIRED: readonly Name[] = ["nif", "rc", "ai", "address"];

function toValues(c: Customer | null, lang: ReturnType<typeof useLang>): Values {
  return {
    name: c?.name ?? "",
    customerType: c?.customerType ?? "individual",
    phone: c?.phone ?? "",
    phone2: c?.phone2 ?? "",
    email: c?.email ?? "",
    address: c?.address ?? "",
    wilaya: c?.wilaya ?? "",
    nif: c?.nif ?? "",
    nis: c?.nis ?? "",
    rc: c?.rc ?? "",
    ai: c?.ai ?? "",
    nin: c?.nin ?? "",
    priceLevelId: c?.priceLevelId ?? "",
    discount: c && c.discountBp > 0 ? formatPercentInput(c.discountBp, lang) : "",
    creditLimit: c?.creditLimit != null ? formatAmountInput(c.creditLimit, lang) : "",
    lang: c?.lang ?? "",
    notes: c?.notes ?? "",
    isFavourite: c?.isFavourite ?? false,
    marketingConsent: c?.marketingConsent ?? false,
  };
}

const opt = (v: string) => (v.trim() === "" ? null : v.trim());
/** Phones and ids typed with Arabic-Indic digits are sent with Western ones. */
const digits = (v: string) => opt(toAsciiDigits(v));

function toInput(v: Values, before: Customer | null): CustomerInput {
  return {
    code: before?.code ?? null,
    name: v.name.trim(),
    customerType: v.customerType,
    phone: digits(v.phone),
    phone2: digits(v.phone2),
    email: opt(v.email),
    address: opt(v.address),
    wilaya: opt(v.wilaya),
    nif: digits(v.nif),
    nis: digits(v.nis),
    rc: opt(v.rc),
    ai: opt(v.ai),
    nin: digits(v.nin),
    idsVerifiedAt: before?.idsVerifiedAt ?? null,
    priceLevelId: opt(v.priceLevelId),
    discountBp: v.discount.trim() === "" ? 0 : (parsePercentBp(v.discount) ?? 0),
    creditLimit: v.creditLimit.trim() === "" ? null : parseAmount(v.creditLimit),
    lang: isLang(v.lang) ? v.lang : null,
    notes: opt(v.notes),
    tags: before?.tags ?? null,
    isFavourite: v.isFavourite,
    marketingConsent: v.marketingConsent,
  };
}

/**
 * Creates or edits a customer (CRM-001/006). The backend checks and cleans every field (phones
 * typed with Arabic digits or +213, NIF lengths from the data pack, a reseller's NIF/RC/AI and
 * address) and names the field it refuses: the error shows under that field. The credit limit is
 * kridi data: without `credit.view` it is neither shown nor changed (the backend keeps it).
 */
export function CustomerForm({
  customer,
  onClose,
  onSaved,
}: {
  customer: Customer | null;
  onClose: () => void;
  onSaved?: (customer: Customer) => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const seesCredit = useSession().data?.permissions.includes("credit.view") ?? false;
  const wilayas = useDatapack().data?.wilayas ?? [];
  const levels = (usePriceLevels().data ?? []).filter(
    (l) => !l.isDefault && (!l.archived || l.id === customer?.priceLevelId),
  );
  const create = useCreateCustomer();
  const update = useUpdateCustomer();
  const [failure, setFailure] = useState<unknown>(null);
  const { register, control, handleSubmit, formState, setError } = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: toValues(customer, lang),
  });
  const reseller = useWatch({ control, name: "customerType" }) === "reseller";
  const err = (name: Name) => {
    const message = formState.errors[name]?.message;
    return typeof message === "string" ? t(message) : undefined;
  };
  const optional = (name: Name) =>
    reseller && RESELLER_REQUIRED.includes(name) ? undefined : t("customers:form.optional");

  const refused = (error: unknown) => {
    const field = SERVER_FIELDS[toAppError(error).field ?? ""];
    if (field) {
      setError(
        field,
        { type: "server", message: errorKey(error, (k) => i18n.exists(k)) },
        { shouldFocus: true },
      );
    } else {
      setFailure(error);
    }
  };
  const saved = (c: Customer) => {
    toast.success(t("customers:form.saved"));
    onClose();
    onSaved?.(c);
  };
  const submit = handleSubmit((values) => {
    setFailure(null);
    const input = toInput(values, customer);
    if (customer) update.mutate({ id: customer.id, input }, { onSuccess: saved, onError: refused });
    else create.mutate(input, { onSuccess: saved, onError: refused });
  });

  const text = (name: Name, labelKey: string, props: { dir?: "ltr" | "auto"; type?: string } = {}) => (
    <Field id={`customer-${name}`} label={t(labelKey)} optional={optional(name)} error={err(name)}>
      <Input
        id={`customer-${name}`}
        dir={props.dir ?? "auto"}
        type={props.type ?? "text"}
        aria-invalid={Boolean(err(name))}
        {...register(name)}
      />
    </Field>
  );

  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={customer ? t("customers:form.editTitle", { name: customer.name }) : t("customers:form.newTitle")}
      description={customer?.code}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            type="submit"
            form="customer-form"
            variant="primary"
            disabled={create.isPending || update.isPending}
          >
            {t("customers:form.save")}
          </Button>
        </>
      }
    >
      <form id="customer-form" noValidate onSubmit={(e) => void submit(e)} className="flex flex-col gap-6">
        <section className="flex flex-col gap-4">
          <h3 className="text-lg font-bold">{t("customers:form.identity")}</h3>
          <div className="grid gap-4 md:grid-cols-2">
            <Field id="customer-name" label={t("customers:fields.name")} error={err("name")}>
              <Input
                id="customer-name"
                dir="auto"
                autoFocus
                aria-invalid={Boolean(err("name"))}
                {...register("name")}
              />
            </Field>
            <Field id="customer-customerType" label={t("customers:fields.type")}>
              <NativeSelect id="customer-customerType" {...register("customerType")}>
                {CUSTOMER_TYPES.map((type) => (
                  <option key={type} value={type}>
                    {t(`customers:types.${type}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            {text("phone", "customers:fields.phone", { dir: "ltr", type: "tel" })}
            {text("phone2", "customers:fields.phone2", { dir: "ltr", type: "tel" })}
            {text("email", "customers:fields.email", { dir: "ltr", type: "email" })}
            <Field id="customer-wilaya" label={t("customers:fields.wilaya")} error={err("wilaya")}>
              <NativeSelect id="customer-wilaya" {...register("wilaya")}>
                <option value="">{t("setup:shop.wilayaNone")}</option>
                {wilayas.map((w) => (
                  <option key={w.code} value={w.code}>
                    {`${w.code} ${localizedName(w, lang)}`}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          </div>
          {text("address", "customers:fields.address")}
        </section>

        <section className="flex flex-col gap-4">
          <h3 className="text-lg font-bold">{t("customers:form.tax")}</h3>
          {reseller ? <p className="text-sm text-ink-2">{t("customers:form.requiredForReseller")}</p> : null}
          <div className="grid gap-4 md:grid-cols-2">
            {text("nif", "customers:fields.nif", { dir: "ltr" })}
            {text("nis", "customers:fields.nis", { dir: "ltr" })}
            {text("rc", "customers:fields.rc", { dir: "ltr" })}
            {text("ai", "customers:fields.ai", { dir: "ltr" })}
            {text("nin", "customers:fields.nin", { dir: "ltr" })}
          </div>
        </section>

        <section className="flex flex-col gap-4">
          <h3 className="text-lg font-bold">{t("customers:form.account")}</h3>
          <div className="grid gap-4 md:grid-cols-2">
            <Field
              id="customer-priceLevelId"
              label={t("customers:fields.priceLevel")}
              error={err("priceLevelId")}
            >
              <NativeSelect id="customer-priceLevelId" {...register("priceLevelId")}>
                <option value="">{t("customers:form.retailPrice")}</option>
                {levels.map((l) => (
                  <option key={l.id} value={l.id}>
                    {localizedName(l, lang)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            <Field
              id="customer-discount"
              label={t("customers:fields.discount")}
              optional={t("customers:form.optional")}
              error={err("discount")}
            >
              <Controller
                control={control}
                name="discount"
                render={({ field }) => (
                  <NumericInput id="customer-discount" value={field.value} onValueChange={field.onChange} />
                )}
              />
            </Field>
            {seesCredit ? (
              <Field
                id="customer-creditLimit"
                label={t("customers:fields.creditLimit")}
                hint={t("customers:form.noLimitHint")}
                error={err("creditLimit")}
              >
                <Controller
                  control={control}
                  name="creditLimit"
                  render={({ field }) => (
                    <NumericInput
                      id="customer-creditLimit"
                      value={field.value}
                      onValueChange={field.onChange}
                    />
                  )}
                />
              </Field>
            ) : null}
            <Field id="customer-lang" label={t("customers:fields.lang")}>
              <NativeSelect id="customer-lang" {...register("lang")}>
                <option value="">{t("customers:form.shopLanguage")}</option>
                {LANGS.map((l) => (
                  <option key={l} value={l} lang={l}>
                    {t(`common:languages.${l}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          </div>
          <Field
            id="customer-notes"
            label={t("customers:fields.notes")}
            optional={t("customers:form.optional")}
            error={err("notes")}
          >
            <textarea
              id="customer-notes"
              dir="auto"
              rows={2}
              maxLength={1000}
              className="w-full rounded-control border border-line bg-surface px-3 py-2 text-base text-ink"
              {...register("notes")}
            />
          </Field>
          {(["isFavourite", "marketingConsent"] as const).map((name) => (
            <Controller
              key={name}
              control={control}
              name={name}
              render={({ field }) => (
                <label className="flex min-h-11 items-center gap-3">
                  <Checkbox checked={field.value} onCheckedChange={(v) => field.onChange(v === true)} />
                  {t(
                    name === "isFavourite"
                      ? "customers:fields.favourite"
                      : "customers:fields.marketingConsent",
                  )}
                </label>
              )}
            />
          ))}
        </section>
        {failure ? (
          <p role="alert" className="text-danger">
            {errorText(failure)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
