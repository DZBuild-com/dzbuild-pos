import { zodResolver } from "@hookform/resolvers/zod";
import { ImageUp, Trash2 } from "lucide-react";
import { type ComponentProps, useMemo, useState } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { z } from "zod";
import { Splash } from "@/components/splash";
import { Button, buttonVariants } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { toAsciiDigits } from "@/lib/digits";
import { formatAmountInput, parseDZD } from "@/lib/format";
import { type Lang, LANGS } from "@/lib/i18n";
import { useDatapack } from "@/lib/ipc/app";
import { useCompany, useSetCompanyLogo, useUpdateCompany } from "@/lib/ipc/company";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { useTillLink } from "@/lib/ipc/lan";
import { useSetSetting, useSetting } from "@/lib/ipc/settings";
import type { Company, CompanyInput, Wilaya } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

const text = z.string().max(200, "validation:tooLong");
const name = z.string().max(80, "validation:tooLong");
/** pos-core `settings`: each social link is at most 120 characters. */
const link = z.string().max(120, "validation:tooLong");
export const SOCIAL_NETWORKS = ["facebook", "instagram", "tiktok", "website"] as const;
type SocialNetwork = (typeof SOCIAL_NETWORKS)[number];
const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/** NIF/NIS as the backend checks them: ASCII digits (Arabic-Indic ones converted), spaces dropped. */
function idDigits(value: string): string {
  return toAsciiDigits(value).replace(/\s/g, "");
}

/** The form schema; `nifLengths` comes from the data pack (15, or 18 for a secondary establishment). */
export function shopSchema(nifLengths: readonly number[]) {
  const idNumber = (message: string) =>
    z.string().refine((v) => {
      const digits = idDigits(v);
      return digits === "" || (/^\d+$/.test(digits) && nifLengths.includes(digits.length));
    }, message);
  return z
    .object({
      nameAr: name,
      nameFr: name,
      nameEn: name,
      legalForm: text,
      capital: z.string().refine((v) => v.trim() === "" || parseDZD(v) !== null, "validation:amountFormat"),
      address: text,
      wilaya: z.string(),
      phone: text,
      email: z.string().refine((v) => v.trim() === "" || EMAIL.test(v.trim()), "validation:emailFormat"),
      rc: text,
      nif: idNumber("validation:nifFormat"),
      // The NIS follows the NIF's digit rule (same lengths from the data pack).
      nis: idNumber("validation:nisFormat"),
      // The owner's national ID (a natural person's shop): 18 digits, or empty.
      nin: z.string().refine((v) => {
        const digits = idDigits(v);
        return digits === "" || /^\d{18}$/.test(digits);
      }, "validation:ninFormat"),
      ai: text,
      rib: text,
      facebook: link,
      instagram: link,
      tiktok: link,
      website: link,
      taxRegime: z.enum(["reel", "ifu"]),
      priceMode: z.enum(["ttc", "ht"]),
      defaultDocLang: z.enum(LANGS),
      receiptHeaderAr: text,
      receiptHeaderFr: text,
      receiptHeaderEn: text,
      receiptFooterAr: text,
      receiptFooterFr: text,
      receiptFooterEn: text,
    })
    .refine((v) => [v.nameAr, v.nameFr, v.nameEn].some((n) => n.trim() !== ""), {
      message: "validation:nameRequired",
      path: ["nameFr"],
    });
}

export type ShopForm = z.infer<ReturnType<typeof shopSchema>>;

type TextField = Exclude<keyof ShopForm, "wilaya" | "taxRegime" | "priceMode" | "defaultDocLang">;

/** The shop settings kept apart from the company record (`company.nin`, `company.social`). */
export interface ShopExtras {
  nin: string;
  social: Partial<Record<SocialNetwork, string>>;
}

export function extrasOf(nin: unknown, social: unknown): ShopExtras {
  const links = typeof social === "object" && social !== null ? (social as Record<string, unknown>) : {};
  return {
    nin: typeof nin === "string" ? nin : "",
    social: Object.fromEntries(
      SOCIAL_NETWORKS.flatMap((n) => (typeof links[n] === "string" ? [[n, links[n]]] : [])),
    ),
  };
}

export function toForm(
  company: Company | null | undefined,
  lang: Lang,
  extras: ShopExtras = { nin: "", social: {} },
): ShopForm {
  const s = (value: string | null | undefined) => value ?? "";
  return {
    nameAr: s(company?.nameAr),
    nameFr: s(company?.nameFr),
    nameEn: s(company?.nameEn),
    legalForm: s(company?.legalForm),
    capital: company?.capital == null ? "" : formatAmountInput(company.capital, lang),
    address: s(company?.address),
    wilaya: s(company?.wilaya),
    phone: s(company?.phone),
    email: s(company?.email),
    rc: s(company?.rc),
    nif: s(company?.nif),
    nis: s(company?.nis),
    nin: extras.nin,
    ai: s(company?.ai),
    rib: s(company?.rib),
    facebook: s(extras.social.facebook),
    instagram: s(extras.social.instagram),
    tiktok: s(extras.social.tiktok),
    website: s(extras.social.website),
    taxRegime: company?.taxRegime ?? "reel",
    priceMode: company?.priceMode ?? "ttc",
    defaultDocLang: company?.defaultDocLang ?? "fr",
    receiptHeaderAr: s(company?.receiptHeaderAr),
    receiptHeaderFr: s(company?.receiptHeaderFr),
    receiptHeaderEn: s(company?.receiptHeaderEn),
    receiptFooterAr: s(company?.receiptFooterAr),
    receiptFooterFr: s(company?.receiptFooterFr),
    receiptFooterEn: s(company?.receiptFooterEn),
  };
}

/** Form values → the command input; NIF, NIS and phone are sent with Western digits. */
export function toInput(v: ShopForm): CompanyInput {
  const opt = (value: string) => (value.trim() === "" ? null : value.trim());
  return {
    nameAr: v.nameAr.trim(),
    nameFr: v.nameFr.trim(),
    nameEn: v.nameEn.trim(),
    legalForm: opt(v.legalForm),
    capital: v.capital.trim() === "" ? null : parseDZD(v.capital),
    address: opt(v.address),
    wilaya: opt(v.wilaya),
    phone: opt(toAsciiDigits(v.phone)),
    email: opt(v.email),
    rc: opt(v.rc),
    nif: opt(idDigits(v.nif)),
    nis: opt(idDigits(v.nis)),
    ai: opt(v.ai),
    rib: opt(v.rib),
    taxRegime: v.taxRegime,
    priceMode: v.priceMode,
    receiptHeaderAr: opt(v.receiptHeaderAr),
    receiptHeaderFr: opt(v.receiptHeaderFr),
    receiptHeaderEn: opt(v.receiptHeaderEn),
    receiptFooterAr: opt(v.receiptFooterAr),
    receiptFooterFr: opt(v.receiptFooterFr),
    receiptFooterEn: opt(v.receiptFooterEn),
    defaultDocLang: v.defaultDocLang,
  };
}

/** Form values → the two settings; the NIN with Western digits, only the links filled in. */
export function toExtras(v: ShopForm): ShopExtras {
  return {
    nin: idDigits(v.nin),
    social: Object.fromEntries(
      SOCIAL_NETWORKS.flatMap((n) => (v[n].trim() === "" ? [] : [[n, v[n].trim()]])),
    ),
  };
}

/** The shop's identity, contact and tax settings. Its activity is changed in Settings › Activité et
 * modules, which applies the activity's template (M8): never written from here. */
export function ShopSettings() {
  const lang = useLang();
  const company = useCompany();
  const nin = useSetting("company.nin");
  const social = useSetting("company.social");
  const datapack = useDatapack();
  if ([company, nin, social, datapack].some((q) => q.isPending)) return <Splash inline />;
  const extras = extrasOf(nin.data, social.data);
  const initial = toForm(company.data, lang, extras);
  return (
    <ShopFormView
      // A save remounts the form with the stored values; a logo change alone keeps unsaved edits.
      key={JSON.stringify(initial)}
      initial={initial}
      extras={extras}
      hasLogo={company.data?.hasLogo ?? false}
      wilayas={datapack.data?.wilayas ?? []}
      nifLengths={datapack.data?.nifLengths ?? []}
    />
  );
}

function ShopFormView({
  initial,
  extras,
  hasLogo,
  wilayas,
  nifLengths,
}: {
  initial: ShopForm;
  /** What the backend holds: a setting is saved only when it changed (each save is audited). */
  extras: ShopExtras;
  hasLogo: boolean;
  wilayas: Wilaya[];
  nifLengths: readonly number[];
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const update = useUpdateCompany();
  const setSetting = useSetSetting();
  // The shop's identity is the main PC's; on a till it is read-only.
  const till = useTillLink();
  const schema = useMemo(() => shopSchema(nifLengths), [nifLengths]);
  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm<ShopForm>({ resolver: zodResolver(schema), defaultValues: initial });

  const fieldError = (field: keyof ShopForm) => {
    const message = errors[field]?.message;
    return message ? t(message) : undefined;
  };
  const textField = (field: TextField, labelKey: string, props: ComponentProps<typeof Input> = {}) => (
    <Field id={`shop-${field}`} label={t(labelKey)} error={fieldError(field)}>
      <Input id={`shop-${field}`} aria-invalid={Boolean(errors[field])} {...props} {...register(field)} />
    </Field>
  );
  const onSubmit = handleSubmit(async (values) => {
    try {
      await update.mutateAsync(toInput(values));
      const next = toExtras(values);
      if (next.nin !== extras.nin) await setSetting.mutateAsync({ key: "company.nin", value: next.nin });
      if (JSON.stringify(next.social) !== JSON.stringify(extras.social)) {
        await setSetting.mutateAsync({ key: "company.social", value: next.social });
      }
      toast.success(t("settings:shop.saved"));
    } catch {
      // shown below from the mutation state
    }
  });
  const failure = update.error ?? setSetting.error;

  return (
    <form noValidate onSubmit={(event) => void onSubmit(event)} className="flex flex-col gap-6">
      <HostOnlyNote />
      <fieldset disabled={till.isTill} className="contents">
        <Card>
          <h2 className="text-xl font-bold">{t("settings:shop.identity")}</h2>
          <div className="grid gap-4 md:grid-cols-3">
            {textField("nameAr", "settings:shop.nameAr", { dir: "rtl", lang: "ar" })}
            {textField("nameFr", "settings:shop.nameFr", { dir: "ltr" })}
            {textField("nameEn", "settings:shop.nameEn", { dir: "ltr" })}
          </div>
          <div className="grid gap-4 md:grid-cols-2">
            {textField("legalForm", "settings:shop.legalForm", { dir: "auto" })}
            {textField("capital", "settings:shop.capital", { dir: "ltr", inputMode: "decimal" })}
          </div>
        </Card>
        <LogoCard hasLogo={hasLogo} readOnly={till.isTill} />
        <Card>
          <h2 className="text-xl font-bold">{t("settings:shop.contact")}</h2>
          {textField("address", "settings:shop.address", { dir: "auto" })}
          <div className="grid gap-4 md:grid-cols-3">
            <Field id="shop-wilaya" label={t("setup:shop.wilaya")}>
              <NativeSelect id="shop-wilaya" {...register("wilaya")}>
                <option value="">{t("setup:shop.wilayaNone")}</option>
                {wilayas.map((w) => (
                  <option key={w.code} value={w.code}>
                    {`${w.code} ${localizedName(w, lang)}`}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            {textField("phone", "setup:shop.phone", { dir: "ltr", type: "tel" })}
            {textField("email", "settings:shop.email", { dir: "ltr", type: "email" })}
          </div>
        </Card>
        <Card>
          <h2 className="text-xl font-bold">{t("settings:shop.social.title")}</h2>
          <div className="grid gap-4 md:grid-cols-2">
            {textField("facebook", "settings:shop.social.facebook", { dir: "ltr" })}
            {textField("instagram", "settings:shop.social.instagram", { dir: "ltr" })}
            {textField("tiktok", "settings:shop.social.tiktok", { dir: "ltr" })}
            {textField("website", "settings:shop.social.website", { dir: "ltr", type: "url" })}
          </div>
        </Card>
        <Card>
          <h2 className="text-xl font-bold">{t("settings:shop.legal")}</h2>
          <div className="grid gap-4 md:grid-cols-2">
            {textField("nif", "settings:shop.nif", { dir: "ltr", inputMode: "numeric" })}
            {textField("nis", "settings:shop.nis", { dir: "ltr", inputMode: "numeric" })}
            {textField("nin", "settings:shop.nin", { dir: "ltr", inputMode: "numeric" })}
            {textField("rc", "settings:shop.rc", { dir: "ltr" })}
            {textField("ai", "settings:shop.ai", { dir: "ltr" })}
            {textField("rib", "settings:shop.rib", { dir: "ltr" })}
          </div>
        </Card>
        <Card>
          <h2 className="text-xl font-bold">{t("settings:shop.tax")}</h2>
          <div className="grid gap-4 md:grid-cols-2">
            <Field id="shop-regime" label={t("setup:shop.regime")}>
              <NativeSelect id="shop-regime" {...register("taxRegime")}>
                <option value="reel">{t("setup:shop.regimeReel")}</option>
                <option value="ifu">{t("setup:shop.regimeIfu")}</option>
              </NativeSelect>
            </Field>
            <Field
              id="shop-price-mode"
              label={t("setup:shop.priceMode")}
              hint={t("settings:shop.priceModeLocked")}
            >
              <NativeSelect id="shop-price-mode" {...register("priceMode")}>
                <option value="ttc">{t("setup:shop.priceTtc")}</option>
                <option value="ht">{t("setup:shop.priceHt")}</option>
              </NativeSelect>
            </Field>
            <Field id="shop-doc-lang" label={t("settings:shop.docLang")}>
              <NativeSelect id="shop-doc-lang" {...register("defaultDocLang")}>
                {LANGS.map((l) => (
                  <option key={l} value={l}>
                    {t(`common:languages.${l}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          </div>
        </Card>
        <Card>
          <h2 className="text-xl font-bold">{t("settings:shop.receipt")}</h2>
          <div className="grid gap-4 md:grid-cols-3">
            {textField("receiptHeaderAr", "settings:shop.headerAr", { dir: "rtl", lang: "ar" })}
            {textField("receiptHeaderFr", "settings:shop.headerFr", { dir: "ltr" })}
            {textField("receiptHeaderEn", "settings:shop.headerEn", { dir: "ltr" })}
            {textField("receiptFooterAr", "settings:shop.footerAr", { dir: "rtl", lang: "ar" })}
            {textField("receiptFooterFr", "settings:shop.footerFr", { dir: "ltr" })}
            {textField("receiptFooterEn", "settings:shop.footerEn", { dir: "ltr" })}
          </div>
        </Card>
      </fieldset>
      {failure ? (
        <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
          {errorText(failure)}
        </p>
      ) : null}
      {till.isTill ? null : (
        <div className="flex justify-end">
          <Button type="submit" variant="primary" disabled={isSubmitting}>
            {t("settings:shop.save")}
          </Button>
        </div>
      )}
    </form>
  );
}

/** Same limits as pos-core `company::set_logo`, checked here so a big file never crosses the IPC. */
const LOGO_MAX_BYTES = 512 * 1024;
const LOGO_TYPES = ["image/png", "image/jpeg"];

/** Saved at once through `company_set_logo`, apart from the form's save button. */
function LogoCard({ hasLogo, readOnly = false }: { hasLogo: boolean; readOnly?: boolean }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const setLogo = useSetCompanyLogo();
  const [refused, setRefused] = useState<string | null>(null);

  const send = async (file: File) => {
    setLogo.reset();
    const refusal =
      file.size > LOGO_MAX_BYTES
        ? "errors:logo_too_large"
        : LOGO_TYPES.includes(file.type)
          ? null
          : "errors:logo_invalid";
    setRefused(refusal);
    if (refusal) return;
    const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
    setLogo.mutate(bytes, { onSuccess: () => toast.success(t("settings:shop.logoSaved")) });
  };
  const failure = refused ? t(refused) : setLogo.error ? errorText(setLogo.error) : null;

  return (
    <Card>
      <h2 className="text-xl font-bold">{t("settings:shop.logo")}</h2>
      <p>{t(hasLogo ? "settings:shop.logoSet" : "settings:shop.logoNone")}</p>
      <p className="text-sm text-ink-2">{t("settings:shop.logoHint")}</p>
      {readOnly ? null : (
        <div className="flex flex-wrap gap-2">
          <label
            className={cn(
              buttonVariants({ variant: "outline" }),
              "cursor-pointer focus-within:ring-3 focus-within:ring-focus",
              setLogo.isPending && "pointer-events-none opacity-50",
            )}
          >
            <input
              type="file"
              accept={LOGO_TYPES.join(",")}
              className="sr-only"
              disabled={setLogo.isPending}
              onChange={(event) => {
                const file = event.target.files?.[0];
                // Cleared so that choosing the same file again still sends it.
                event.target.value = "";
                if (file) void send(file);
              }}
            />
            <ImageUp aria-hidden />
            {t("settings:shop.logoChoose")}
          </label>
          {hasLogo ? (
            <Button
              variant="danger"
              disabled={setLogo.isPending}
              onClick={() => {
                setRefused(null);
                setLogo.mutate(null, { onSuccess: () => toast.success(t("settings:shop.logoRemoved")) });
              }}
            >
              <Trash2 aria-hidden />
              {t("settings:shop.logoRemove")}
            </Button>
          ) : null}
        </div>
      )}
      {failure ? (
        <p role="alert" className="text-sm text-danger">
          {failure}
        </p>
      ) : null}
    </Card>
  );
}
