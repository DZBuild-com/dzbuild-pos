import "@/documents/documents.css";
import { zodResolver } from "@hookform/resolvers/zod";
import { memo, useDeferredValue, useEffect, useMemo, useState } from "react";
import {
  type Control,
  Controller,
  type UseFormReturn,
  useForm,
  useFormState,
  useWatch,
} from "react-hook-form";
import { useTranslation } from "react-i18next";
import { useLocation } from "react-router";
import { toast } from "sonner";
import { z } from "zod";
import { LanguageToggle } from "@/components/language-toggle";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { Splash } from "@/components/splash";
import { Button } from "@/components/ui/button";
import { useTillLink } from "@/lib/ipc/lan";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import invoiceAr from "@/documents/__fixtures__/invoice.ar.json";
import invoiceEn from "@/documents/__fixtures__/invoice.en.json";
import invoiceFr from "@/documents/__fixtures__/invoice.fr.json";
import { overlay } from "@/documents/overlay";
import { ScaledPage } from "@/features/documents/PreviewDialog";
import { useCan } from "@/features/stock/stock-parts";
import { BlInvoiceCard, MinOrderPolicy } from "@/features/wholesale/BlInvoiceCard";
import { isLang, type Lang, LANGS } from "@/lib/i18n";
import { useCategories } from "@/lib/ipc/catalog";
import { type SettingKey, useSetSetting, useSetting } from "@/lib/ipc/settings";
import type {
  Accent,
  Category,
  DocumentLayout,
  DocumentPrintModel,
  DocumentTexts,
  HostPage,
  JsonValue,
  Paper,
  Trilingual,
} from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { DocumentPrintersCard } from "./DocumentPrintersCard";

/** pos-core `billing::settings` and `settings::REGISTRY` limits. */
const HEADER_MAX = 200;
const TERMS_MAX = 600;
const ACTIVITY_MAX = 120;
const AE_CARD_MAX = 40;
const ACCENTS = ["brand", "ink", "info", "ok"] as const satisfies readonly Accent[];
const PAPERS = ["a4", "a5"] as const satisfies readonly Paper[];
const GUARDS = ["off", "consumer", "categories"] as const;
const LABEL_MODES = ["activity", "categories"] as const;
const TEXTS = [
  ["header", HEADER_MAX],
  ["footer", HEADER_MAX],
  ["terms", TERMS_MAX],
  ["warranty", TERMS_MAX],
] as const satisfies readonly (readonly [keyof DocumentTexts, number])[];

/** The sample invoice of the preview, in each language (the print templates' fixtures). */
const SAMPLES: Record<Lang, DocumentPrintModel> = {
  ar: invoiceAr as unknown as DocumentPrintModel,
  fr: invoiceFr as unknown as DocumentPrintModel,
  en: invoiceEn as unknown as DocumentPrintModel,
};

/** The screen's settings, as the backend holds them. */
const KEYS = [
  "documents.layout",
  "documents.texts",
  "company.activity",
  "company.ae_card",
  "documents.quote_validity_days",
  "documents.invoice_due_days",
  "documents.arabic_guard",
  "documents.arabic_guard_categories",
  "labels.arabic_categories",
] as const satisfies readonly SettingKey[];
type DocKey = (typeof KEYS)[number];
type Stored = Record<DocKey, unknown>;

const tri = (max: number) =>
  z.object({
    ar: z.string().max(max, "validation:tooLong"),
    fr: z.string().max(max, "validation:tooLong"),
    en: z.string().max(max, "validation:tooLong"),
  });

const layoutSchema = z.object({
  layout: z.object({
    accent: z.enum(ACCENTS),
    showLogo: z.boolean(),
    paper: z.enum(PAPERS),
    secondLang: z.enum(LANGS).nullable(),
    showDeliveryPrices: z.boolean(),
    showStampBox: z.boolean(),
  }),
});
const textsSchema = z.object({
  texts: z.object({
    header: tri(HEADER_MAX),
    footer: tri(HEADER_MAX),
    terms: tri(TERMS_MAX),
    warranty: tri(TERMS_MAX),
  }),
});
const identitySchema = z.object({
  activity: tri(ACTIVITY_MAX),
  aeCard: z.string().max(AE_CARD_MAX, "validation:tooLong"),
});
const days = (min: number, message: string) =>
  z.string().refine((v) => /^\d{1,3}$/.test(v) && Number(v) >= min && Number(v) <= 365, message);
const rulesSchema = z.object({
  quoteDays: days(1, "validation:quoteDaysRange"),
  dueDays: days(0, "validation:daysRange"),
});
const arabicSchema = z.object({
  guard: z.enum(GUARDS),
  guardCategories: z.array(z.string()),
  labelsMode: z.enum(LABEL_MODES),
  labelCategories: z.array(z.string()),
});

type LayoutForm = z.infer<typeof layoutSchema>;
type TextsForm = z.infer<typeof textsSchema>;
type IdentityForm = z.infer<typeof identitySchema>;
type RulesForm = z.infer<typeof rulesSchema>;
type ArabicForm = z.infer<typeof arabicSchema>;
type Texts = TextsForm["texts"];
type Tri = Texts["header"];

const DEFAULT_LAYOUT: DocumentLayout = {
  accent: "brand",
  showLogo: true,
  paper: "a4",
  secondLang: null,
  showDeliveryPrices: true,
  showStampBox: true,
};

const str = (value: unknown): string => (typeof value === "string" ? value : "");

function oneOf<T extends string>(value: unknown, allowed: readonly T[], fallback: T): T {
  return allowed.find((a) => a === value) ?? fallback;
}

function whole(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isSafeInteger(value) ? value : fallback;
}

function idsOf(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((id): id is string => typeof id === "string") : [];
}

function layoutOf(value: unknown): DocumentLayout {
  const parsed = layoutSchema.shape.layout.safeParse(value);
  return parsed.success ? parsed.data : DEFAULT_LAYOUT;
}

function triOf(value: unknown): Tri {
  const t = (value ?? {}) as Trilingual;
  return { ar: str(t.ar), fr: str(t.fr), en: str(t.en) };
}

function textsOf(value: unknown): Texts {
  const t = (value ?? {}) as DocumentTexts;
  return {
    header: triOf(t.header),
    footer: triOf(t.footer),
    terms: triOf(t.terms),
    warranty: triOf(t.warranty),
  };
}

/** JSON with object keys sorted: a value equals the stored one whatever order the backend wrote. */
function canonical(value: unknown): string {
  return JSON.stringify(value, (_key, v: unknown) =>
    v && typeof v === "object" && !Array.isArray(v)
      ? Object.fromEntries(Object.entries(v).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
      : v,
  );
}

/** Saves a card's settings: only those that differ from what the backend holds (each save is an
 * audit row), stopping at the first refusal, which `errorOf` gives for the field it concerns. */
function useSaveSettings(stored: Stored) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const setSetting = useSetSetting();
  const [failure, setFailure] = useState<{ key: DocKey; error: unknown } | null>(null);
  const save = async (changes: { key: DocKey; value: JsonValue }[]) => {
    setFailure(null);
    for (const { key, value } of changes) {
      if (canonical(value) === canonical(stored[key])) continue;
      try {
        await setSetting.mutateAsync({ key, value });
      } catch (error) {
        setFailure({ key, error });
        return;
      }
    }
    toast.success(t("settings:documents.saved"));
  };
  const errorOf = (key: DocKey) => (failure?.key === key ? errorText(failure.error) : undefined);
  return { save, errorOf };
}

/** Réglages › Documents (CFG-001/002/005, DOC-023/024, DOC-034): layout, texts, fiscal identity,
 * rules, the Arabic of labels and documents, and this PC's A4 printers, beside a live preview. */
export function DocumentsSettings() {
  const settings = {
    "documents.layout": useSetting("documents.layout"),
    "documents.texts": useSetting("documents.texts"),
    "company.activity": useSetting("company.activity"),
    "company.ae_card": useSetting("company.ae_card"),
    "documents.quote_validity_days": useSetting("documents.quote_validity_days"),
    "documents.invoice_due_days": useSetting("documents.invoice_due_days"),
    "documents.arabic_guard": useSetting("documents.arabic_guard"),
    "documents.arabic_guard_categories": useSetting("documents.arabic_guard_categories"),
    "labels.arabic_categories": useSetting("labels.arabic_categories"),
  } satisfies Record<DocKey, unknown>;
  if (Object.values(settings).some((q) => q.isPending)) return <Splash inline />;
  const stored = Object.fromEntries(KEYS.map((key) => [key, settings[key].data])) as Stored;
  return <DocumentsSettingsView stored={stored} />;
}

/** `stored` follows the backend (each save updates its query); the forms start from it once. The
 * layout, texts and identity forms live here so the preview can watch their unsaved values; only
 * the preview re-renders when they change, never the other cards. */
function DocumentsSettingsView({ stored }: { stored: Stored }) {
  const can = useCan();
  // The shop's document settings are the main PC's; on a till they are read-only.
  const till = useTillLink();
  const uiLang = useLang();
  const [lang, setLang] = useState<Lang>(uiLang);
  const layoutForm = useForm<LayoutForm>({
    resolver: zodResolver(layoutSchema),
    defaultValues: { layout: layoutOf(stored["documents.layout"]) },
  });
  const textsForm = useForm<TextsForm>({
    resolver: zodResolver(textsSchema),
    defaultValues: { texts: textsOf(stored["documents.texts"]) },
  });
  const identityForm = useForm<IdentityForm>({
    resolver: zodResolver(identitySchema),
    defaultValues: { activity: triOf(stored["company.activity"]), aeCard: str(stored["company.ae_card"]) },
  });
  // A link to one card (the mentions panel's "#documents-identity") lands on it.
  const { hash } = useLocation();
  useEffect(() => {
    if (hash) document.getElementById(hash.slice(1))?.scrollIntoView?.({ block: "start" });
  }, [hash]);
  return (
    <div className="grid items-start gap-6 xl:grid-cols-[minmax(0,1fr)_22rem]">
      <div className="flex min-w-0 flex-col gap-6">
        <HostOnlyNote />
        <fieldset disabled={till.isTill} className="contents">
          <LayoutCard form={layoutForm} stored={stored} />
          <TextsCard form={textsForm} stored={stored} lang={lang} onLang={setLang} />
          <IdentityCard form={identityForm} stored={stored} />
          <RulesCard stored={stored} />
          {/* M8 wholesale (CFG-003): delivery notes standing as invoices, while the module is on */}
          <BlInvoiceCard />
          <MinOrderPolicy />
          <ArabicCard stored={stored} />
        </fieldset>
        {can("settings.hardware") ? (
          <DocumentPrintersCard defaultPaper={layoutOf(stored["documents.layout"]).paper} />
        ) : null}
      </div>
      <Preview
        lang={lang}
        onLang={setLang}
        layoutControl={layoutForm.control}
        textsControl={textsForm.control}
        identityControl={identityForm.control}
      />
    </div>
  );
}

function SaveButton({ label, pending }: { label: string; pending: boolean }) {
  const till = useTillLink();
  if (till.isTill) return null;
  return (
    <div className="flex justify-end">
      <Button type="submit" disabled={pending}>
        {label}
      </Button>
    </div>
  );
}

function Refusal({ text }: { text: string | undefined }) {
  return text ? (
    <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
      {text}
    </p>
  ) : null;
}

function LayoutCard({ form, stored }: { form: UseFormReturn<LayoutForm>; stored: Stored }) {
  const { t } = useTranslation();
  const { save, errorOf } = useSaveSettings(stored);
  const { control, handleSubmit } = form;
  // The card's own subscription: a submit re-renders this card, not the whole screen.
  const { isSubmitting } = useFormState({ control });
  const onSubmit = handleSubmit((v) => save([{ key: "documents.layout", value: v.layout }]));
  const check = (
    name: "layout.showLogo" | "layout.showDeliveryPrices" | "layout.showStampBox",
    label: string,
  ) => (
    <Controller
      name={name}
      control={control}
      render={({ field }) => (
        <label className="flex min-h-11 items-center gap-3 font-semibold">
          <Checkbox checked={field.value} onCheckedChange={(v) => field.onChange(v === true)} />
          {label}
        </label>
      )}
    />
  );
  return (
    <Card>
      <form noValidate onSubmit={(event) => void onSubmit(event)} className="flex flex-col gap-4">
        <h2 className="text-xl font-bold">{t("settings:documents.layout.title")}</h2>
        <Controller
          name="layout.accent"
          control={control}
          render={({ field }) => (
            <RadioCards
              legend={t("settings:documents.layout.accent")}
              name="documents-accent"
              value={field.value}
              onChange={field.onChange}
              options={ACCENTS.map((accent) => ({
                value: accent,
                label: t(`settings:documents.layout.accents.${accent}`),
              }))}
            />
          )}
        />
        <Controller
          name="layout.paper"
          control={control}
          render={({ field }) => (
            <RadioCards
              legend={t("settings:documents.layout.paper")}
              name="documents-paper"
              value={field.value}
              onChange={field.onChange}
              options={PAPERS.map((paper) => ({
                value: paper,
                label: t(`settings:documents.layout.papers.${paper}`),
              }))}
            />
          )}
        />
        <Field id="documents-second-lang" label={t("settings:documents.layout.secondLang")}>
          <Controller
            name="layout.secondLang"
            control={control}
            render={({ field }) => (
              <NativeSelect
                id="documents-second-lang"
                value={field.value ?? ""}
                onChange={(event) => field.onChange(isLang(event.target.value) ? event.target.value : null)}
              >
                <option value="">{t("settings:documents.layout.secondLangNone")}</option>
                {LANGS.map((l) => (
                  <option key={l} value={l}>
                    {t(`common:languages.${l}`)}
                  </option>
                ))}
              </NativeSelect>
            )}
          />
        </Field>
        {check("layout.showLogo", t("settings:documents.layout.logo"))}
        {check("layout.showDeliveryPrices", t("settings:documents.layout.deliveryPrices"))}
        {check("layout.showStampBox", t("settings:documents.layout.stampBox"))}
        <Refusal text={errorOf("documents.layout")} />
        <SaveButton label={t("settings:documents.layout.save")} pending={isSubmitting} />
      </form>
    </Card>
  );
}

function TextsCard({
  form,
  stored,
  lang,
  onLang,
}: {
  form: UseFormReturn<TextsForm>;
  stored: Stored;
  lang: Lang;
  onLang: (lang: Lang) => void;
}) {
  const { t } = useTranslation();
  const { save, errorOf } = useSaveSettings(stored);
  const { control, register, handleSubmit } = form;
  const { errors, isSubmitting } = useFormState({ control });
  const texts = useWatch({ control, name: "texts" });
  const onSubmit = handleSubmit((v) => save([{ key: "documents.texts", value: v.texts }]));
  const langName = t(`common:languages.${lang}`);
  return (
    <Card>
      <form noValidate onSubmit={(event) => void onSubmit(event)} className="flex flex-col gap-4">
        <h2 className="text-xl font-bold">{t("settings:documents.texts.title")}</h2>
        <div
          role="tablist"
          aria-label={t("common:languages.label")}
          className="flex flex-wrap gap-2 border-b border-line pb-2"
        >
          {LANGS.map((l) => (
            <button
              key={l}
              type="button"
              role="tab"
              id={`documents-texts-tab-${l}`}
              aria-selected={lang === l}
              aria-controls="documents-texts-panel"
              lang={l}
              onClick={() => onLang(l)}
              className={cn(
                "min-h-11 rounded-control px-4 font-semibold",
                lang === l ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
              )}
            >
              {t(`common:languages.${l}`)}
            </button>
          ))}
        </div>
        <div
          role="tabpanel"
          id="documents-texts-panel"
          aria-labelledby={`documents-texts-tab-${lang}`}
          className="flex flex-col gap-4"
        >
          {TEXTS.map(([key, max]) => {
            const id = `documents-${key}-${lang}`;
            return (
              <Field
                key={id}
                id={id}
                label={t(`settings:documents.texts.${key}`, { lang: langName })}
                error={errors.texts?.[key]?.[lang]?.message ? t(errors.texts[key][lang].message) : undefined}
              >
                <textarea
                  id={id}
                  lang={lang}
                  dir={lang === "ar" ? "rtl" : "ltr"}
                  rows={max > HEADER_MAX ? 4 : 2}
                  maxLength={max}
                  aria-describedby={`${id}-count`}
                  className="w-full rounded-control border border-line bg-surface px-3 py-2 text-base text-ink"
                  {...register(`texts.${key}.${lang}`)}
                />
                <p id={`${id}-count`} className="text-sm text-ink-2 tabular-nums">
                  <bdi dir="ltr">
                    {t("settings:documents.texts.count", { used: texts[key][lang].length, max })}
                  </bdi>
                </p>
              </Field>
            );
          })}
        </div>
        <Refusal text={errorOf("documents.texts")} />
        <SaveButton label={t("settings:documents.texts.save")} pending={isSubmitting} />
      </form>
    </Card>
  );
}

function IdentityCard({ form, stored }: { form: UseFormReturn<IdentityForm>; stored: Stored }) {
  const { t } = useTranslation();
  const { save, errorOf } = useSaveSettings(stored);
  const { control, register, handleSubmit } = form;
  const { errors, isSubmitting } = useFormState({ control });
  const onSubmit = handleSubmit((v) =>
    save([
      { key: "company.activity", value: v.activity },
      { key: "company.ae_card", value: v.aeCard.trim() },
    ]),
  );
  const err = (message: string | undefined) => (message ? t(message) : undefined);
  return (
    <Card id="documents-identity" className="scroll-mt-4">
      <form noValidate onSubmit={(event) => void onSubmit(event)} className="flex flex-col gap-4">
        <h2 className="text-xl font-bold">{t("settings:documents.identity.title")}</h2>
        <p className="text-sm text-ink-2">{t("settings:documents.identity.activityHint")}</p>
        <div className="grid gap-4 md:grid-cols-3">
          {LANGS.map((l) => (
            <Field
              key={l}
              id={`documents-activity-${l}`}
              label={t("settings:documents.identity.activity", { lang: t(`common:languages.${l}`) })}
              error={err(errors.activity?.[l]?.message)}
            >
              <Input
                id={`documents-activity-${l}`}
                lang={l}
                dir={l === "ar" ? "rtl" : "ltr"}
                maxLength={ACTIVITY_MAX}
                {...register(`activity.${l}`)}
              />
            </Field>
          ))}
        </div>
        <Refusal text={errorOf("company.activity")} />
        <Field
          id="documents-ae-card"
          label={t("settings:documents.identity.aeCard")}
          hint={t("settings:documents.identity.aeCardHint")}
          error={err(errors.aeCard?.message) ?? errorOf("company.ae_card")}
        >
          <Input id="documents-ae-card" dir="ltr" maxLength={AE_CARD_MAX} {...register("aeCard")} />
        </Field>
        <SaveButton label={t("settings:documents.identity.save")} pending={isSubmitting} />
      </form>
    </Card>
  );
}

function RulesCard({ stored }: { stored: Stored }) {
  const { t } = useTranslation();
  const { save, errorOf } = useSaveSettings(stored);
  const {
    control,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm<RulesForm>({
    resolver: zodResolver(rulesSchema),
    defaultValues: {
      quoteDays: String(whole(stored["documents.quote_validity_days"], 30)),
      dueDays: String(whole(stored["documents.invoice_due_days"], 30)),
    },
  });
  const onSubmit = handleSubmit((v) =>
    save([
      { key: "documents.quote_validity_days", value: Number(v.quoteDays) },
      { key: "documents.invoice_due_days", value: Number(v.dueDays) },
    ]),
  );
  const dayField = (name: keyof RulesForm, key: DocKey, id: string, label: string) => (
    <Field id={id} label={label} error={errors[name]?.message ? t(errors[name].message) : errorOf(key)}>
      <Controller
        name={name}
        control={control}
        render={({ field }) => (
          <NumericInput
            id={id}
            kind="integer"
            maxLength={3}
            aria-invalid={Boolean(errors[name])}
            value={field.value}
            onValueChange={field.onChange}
          />
        )}
      />
    </Field>
  );
  return (
    <Card>
      <form noValidate onSubmit={(event) => void onSubmit(event)} className="flex flex-col gap-4">
        <h2 className="text-xl font-bold">{t("settings:documents.rules.title")}</h2>
        <div className="grid gap-4 md:grid-cols-2">
          {dayField(
            "quoteDays",
            "documents.quote_validity_days",
            "documents-quote-days",
            t("settings:documents.rules.quoteDays"),
          )}
          {dayField(
            "dueDays",
            "documents.invoice_due_days",
            "documents-due-days",
            t("settings:documents.rules.dueDays"),
          )}
        </div>
        <SaveButton label={t("settings:documents.rules.save")} pending={isSubmitting} />
      </form>
    </Card>
  );
}

/** DOC-034 (décret exécutif 13-378): the documents' guard (M3) and the labels' (M4) side by side. */
function ArabicCard({ stored }: { stored: Stored }) {
  const { t } = useTranslation();
  const { save, errorOf } = useSaveSettings(stored);
  const categories = (useCategories().data ?? [])
    .filter((c) => !c.archived)
    .sort((a, b) => a.sortOrder - b.sortOrder);
  const labels = stored["labels.arabic_categories"];
  const {
    control,
    handleSubmit,
    formState: { isSubmitting },
  } = useForm<ArabicForm>({
    resolver: zodResolver(arabicSchema),
    defaultValues: {
      guard: oneOf(stored["documents.arabic_guard"], GUARDS, "consumer"),
      guardCategories: idsOf(stored["documents.arabic_guard_categories"]),
      // `null`: the shop's activity decides (pos-core `labels::ArabicGuard`).
      labelsMode: Array.isArray(labels) ? "categories" : "activity",
      labelCategories: idsOf(labels),
    },
  });
  const [guard, labelsMode] = useWatch({ control, name: ["guard", "labelsMode"] });
  const onSubmit = handleSubmit((v) =>
    save([
      { key: "documents.arabic_guard", value: v.guard },
      { key: "documents.arabic_guard_categories", value: v.guardCategories },
      { key: "labels.arabic_categories", value: v.labelsMode === "activity" ? null : v.labelCategories },
    ]),
  );
  return (
    <Card>
      <form noValidate onSubmit={(event) => void onSubmit(event)} className="flex flex-col gap-4">
        <h2 className="text-xl font-bold">{t("settings:documents.arabic.title")}</h2>
        <p className="text-sm text-ink-2">{t("settings:documents.arabic.hint")}</p>
        <Controller
          name="guard"
          control={control}
          render={({ field }) => (
            <RadioCards
              legend={t("settings:documents.arabic.documents")}
              name="documents-arabic-guard"
              value={field.value}
              onChange={field.onChange}
              options={GUARDS.map((g) => ({ value: g, label: t(`settings:documents.arabic.guards.${g}`) }))}
            />
          )}
        />
        <Refusal text={errorOf("documents.arabic_guard")} />
        {guard === "categories" ? (
          <Controller
            name="guardCategories"
            control={control}
            render={({ field }) => (
              <CategoryChecklist
                legend={t("settings:documents.arabic.documentCategories")}
                categories={categories}
                value={field.value}
                onChange={field.onChange}
                error={errorOf("documents.arabic_guard_categories")}
              />
            )}
          />
        ) : null}
        <Controller
          name="labelsMode"
          control={control}
          render={({ field }) => (
            <RadioCards
              legend={t("settings:documents.arabic.labels")}
              name="labels-arabic"
              value={field.value}
              onChange={field.onChange}
              options={LABEL_MODES.map((mode) => ({
                value: mode,
                label: t(`settings:documents.arabic.labelModes.${mode}`),
                ...(mode === "activity" ? { hint: t("settings:documents.arabic.labelActivityHint") } : {}),
              }))}
            />
          )}
        />
        {labelsMode === "categories" ? (
          <Controller
            name="labelCategories"
            control={control}
            render={({ field }) => (
              <CategoryChecklist
                legend={t("settings:documents.arabic.labelCategories")}
                categories={categories}
                value={field.value}
                onChange={field.onChange}
                error={errorOf("labels.arabic_categories")}
              />
            )}
          />
        ) : (
          <Refusal text={errorOf("labels.arabic_categories")} />
        )}
        <SaveButton label={t("settings:documents.arabic.save")} pending={isSubmitting} />
      </form>
    </Card>
  );
}

/** Ticks categories by name in the UI language; ids of archived categories stay selected. */
function CategoryChecklist({
  legend,
  categories,
  value,
  onChange,
  error,
}: {
  legend: string;
  categories: Category[];
  value: string[];
  onChange: (ids: string[]) => void;
  error: string | undefined;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const toggle = (id: string, on: boolean) =>
    onChange(on ? [...value.filter((v) => v !== id), id] : value.filter((v) => v !== id));
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="mb-1 font-semibold">{legend}</legend>
      {categories.length === 0 ? (
        <p className="text-ink-2">{t("settings:documents.arabic.noCategories")}</p>
      ) : (
        <div className="grid gap-x-4 sm:grid-cols-2">
          {categories.map((c) => (
            <label key={c.id} className="flex min-h-11 items-center gap-3">
              <Checkbox checked={value.includes(c.id)} onCheckedChange={(v) => toggle(c.id, v === true)} />
              {localizedName(c, lang)}
            </label>
          ))}
        </div>
      )}
      {error ? (
        <p role="alert" className="text-sm text-danger">
          {error}
        </p>
      ) : null}
    </fieldset>
  );
}

/** The sample sheet, laid out again only when what it shows changes. */
const Sheet = memo(ScaledPage);

/** The sample invoice in the chosen language with the unsaved layout, texts and activity laid
 * over it; nothing is sent to Rust. The values are deferred: a keystroke is shown in its field at
 * once and the sheet follows when the PC has time (a Celeron lays out an A4 page slowly). */
function Preview({
  lang,
  onLang,
  layoutControl,
  textsControl,
  identityControl,
}: {
  lang: Lang;
  onLang: (lang: Lang) => void;
  layoutControl: Control<LayoutForm>;
  textsControl: Control<TextsForm>;
  identityControl: Control<IdentityForm>;
}) {
  const { t } = useTranslation();
  const layout = useDeferredValue(useWatch({ control: layoutControl, name: "layout" }));
  const texts = useDeferredValue(useWatch({ control: textsControl, name: "texts" }));
  const activity = useDeferredValue(useWatch({ control: identityControl, name: "activity" }));
  const sampleLang = useDeferredValue(lang);
  const page = useMemo<HostPage>(
    () => ({
      template: "document",
      model: overlay(SAMPLES[sampleLang], { layout, texts, activity, lang: sampleLang }),
    }),
    [layout, texts, activity, sampleLang],
  );
  return (
    <aside aria-labelledby="documents-preview-title" className="flex flex-col gap-3 xl:sticky xl:top-4">
      <h2 id="documents-preview-title" className="text-xl font-bold">
        {t("settings:documents.preview")}
      </h2>
      <LanguageToggle value={lang} onChange={onLang} />
      <p className="text-sm text-ink-2">{t("settings:documents.previewHint")}</p>
      <div data-testid="document-preview">
        <Sheet page={page} paper={layout.paper} />
      </div>
    </aside>
  );
}
