import { zodResolver } from "@hookform/resolvers/zod";
import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { Controller, useFieldArray, useForm, useWatch } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { z } from "zod";
import { DateRangePicker } from "@/components/date-range";
import { NumericInput } from "@/components/numeric-input";
import { Splash } from "@/components/splash";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { isDay } from "@/lib/dates";
import { formatAmountInput } from "@/lib/format";
import { type SettingKey, useSetSetting, useSetting } from "@/lib/ipc/settings";
import { useTillLink } from "@/lib/ipc/lan";
import type { JsonValue } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** pos-core `settings::REGISTRY` limits. */
const MAX_AMOUNT = 100_000_000;
const MAX_SOLDES_WINDOWS = 12;
const ROUNDINGS = ["0", "500", "1000"] as const;
const STAMP_MODES = ["charge_customer", "absorb", "off"] as const;
const NEGATIVE_POLICIES = ["allow", "warn", "block"] as const;
const EXPIRED_LOTS = ["warn", "block"] as const;
/** CFG-008: the business day turns at 00:00 to 06:00 (pos-core `settings::REGISTRY`, 0–6). */
const DAY_CUTOFFS = ["0", "1", "2", "3", "4", "5", "6"] as const;

const amount = z
  .string()
  .refine((v) => parseAmount(v) !== null, "validation:amountFormat")
  .refine((v) => (parseAmount(v) ?? 0) <= MAX_AMOUNT, "validation:amountMax");

const soldesWindow = z
  .object({ from: z.string(), to: z.string() })
  .refine((w) => isDay(w.from) && isDay(w.to), { message: "validation:required", path: ["to"] })
  .refine((w) => !isDay(w.from) || !isDay(w.to) || w.from <= w.to, {
    message: "validation:rangeOrder",
    path: ["to"],
  });

const rulesSchema = z.object({
  maxChange: amount,
  cashRounding: z.enum(ROUNDINGS),
  chequeRequiresCustomer: z.boolean(),
  stampMode: z.enum(STAMP_MODES),
  varianceThreshold: amount,
  blindClose: z.boolean(),
  dayCutoff: z.enum(DAY_CUTOFFS),
  negativePolicy: z.enum(NEGATIVE_POLICIES),
  expiredLots: z.enum(EXPIRED_LOTS),
  expiryAlertDays: z.string().refine((v) => /^\d{1,3}$/.test(v) && Number(v) <= 365, "validation:daysRange"),
  eanPrefix: z.string().regex(/^2\d{2}$/, "validation:eanPrefix"),
  soldes: z.array(soldesWindow).max(MAX_SOLDES_WINDOWS),
});

type RulesForm = z.infer<typeof rulesSchema>;

/** The screen's settings, in the order they are saved. */
const KEYS = [
  "sales.max_change",
  "sales.cash_rounding",
  "payments.cheque_requires_customer",
  "stamp_duty.mode",
  "cash.variance_threshold",
  "cash.blind_close",
  "sales.day_cutoff_hour",
  "stock.negative_policy",
  "stock.expired_lots",
  "stock.expiry_alert_days",
  "products.internal_ean_prefix",
  "promotions.soldes_windows",
] as const satisfies readonly SettingKey[];
type RuleKey = (typeof KEYS)[number];
type Stored = Partial<Record<RuleKey, unknown>>;

function oneOf<T extends string>(value: unknown, allowed: readonly T[], fallback: T): T {
  return allowed.find((a) => a === value) ?? fallback;
}

function whole(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isSafeInteger(value) ? value : fallback;
}

function windowsOf(value: unknown): RulesForm["soldes"] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((w: unknown) => {
    const { from, to } = (w ?? {}) as { from?: unknown; to?: unknown };
    return typeof from === "string" && typeof to === "string" ? [{ from, to }] : [];
  });
}

/** Stored values (the backend's defaults when unset) → the form. */
function toForm(stored: Stored, lang: ReturnType<typeof useLang>): RulesForm {
  return {
    maxChange: formatAmountInput(whole(stored["sales.max_change"], 0), lang),
    cashRounding: oneOf(String(stored["sales.cash_rounding"]), ROUNDINGS, "0"),
    chequeRequiresCustomer: stored["payments.cheque_requires_customer"] === true,
    stampMode: oneOf(stored["stamp_duty.mode"], STAMP_MODES, "charge_customer"),
    varianceThreshold: formatAmountInput(whole(stored["cash.variance_threshold"], 50_000), lang),
    blindClose: stored["cash.blind_close"] === true,
    dayCutoff: oneOf(String(whole(stored["sales.day_cutoff_hour"], 0)), DAY_CUTOFFS, "0"),
    negativePolicy: oneOf(stored["stock.negative_policy"], NEGATIVE_POLICIES, "warn"),
    expiredLots: oneOf(stored["stock.expired_lots"], EXPIRED_LOTS, "warn"),
    expiryAlertDays: String(whole(stored["stock.expiry_alert_days"], 30)),
    eanPrefix:
      typeof stored["products.internal_ean_prefix"] === "string"
        ? stored["products.internal_ean_prefix"]
        : "200",
    soldes: windowsOf(stored["promotions.soldes_windows"]),
  };
}

/** The form → one JSON value per setting; amounts in centimes (never a float). */
function toSettings(v: RulesForm): Record<RuleKey, JsonValue> {
  return {
    "sales.max_change": parseAmount(v.maxChange) ?? 0,
    "sales.cash_rounding": Number(v.cashRounding),
    "payments.cheque_requires_customer": v.chequeRequiresCustomer,
    "stamp_duty.mode": v.stampMode,
    "cash.variance_threshold": parseAmount(v.varianceThreshold) ?? 0,
    "cash.blind_close": v.blindClose,
    "sales.day_cutoff_hour": Number(v.dayCutoff),
    "stock.negative_policy": v.negativePolicy,
    "stock.expired_lots": v.expiredLots,
    "stock.expiry_alert_days": Number(v.expiryAlertDays),
    "products.internal_ean_prefix": v.eanPrefix,
    "promotions.soldes_windows": v.soldes.map(({ from, to }) => ({ from, to })),
  };
}

/** Sales, cash, stock and soldes rules (PAY-016/017/020, CSH-004/005, STK-017, PRD-003/053,
 * PRM-021): the M1 settings left without a screen and the M2 ones. */
export function SalesRulesSettings() {
  const lang = useLang();
  const settings = {
    "sales.max_change": useSetting("sales.max_change"),
    "sales.cash_rounding": useSetting("sales.cash_rounding"),
    "payments.cheque_requires_customer": useSetting("payments.cheque_requires_customer"),
    "stamp_duty.mode": useSetting("stamp_duty.mode"),
    "cash.variance_threshold": useSetting("cash.variance_threshold"),
    "cash.blind_close": useSetting("cash.blind_close"),
    "sales.day_cutoff_hour": useSetting("sales.day_cutoff_hour"),
    "stock.negative_policy": useSetting("stock.negative_policy"),
    "stock.expired_lots": useSetting("stock.expired_lots"),
    "stock.expiry_alert_days": useSetting("stock.expiry_alert_days"),
    "products.internal_ean_prefix": useSetting("products.internal_ean_prefix"),
    "promotions.soldes_windows": useSetting("promotions.soldes_windows"),
  } satisfies Record<RuleKey, unknown>;
  if (Object.values(settings).some((q) => q.isPending)) return <Splash inline />;
  const stored: Stored = Object.fromEntries(KEYS.map((key) => [key, settings[key].data]));
  return <RulesFormView stored={stored} initial={toForm(stored, lang)} />;
}

/** `stored` follows the backend (each save updates its query); `initial` fills the form once. */
function RulesFormView({ stored, initial }: { stored: Stored; initial: RulesForm }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const setSetting = useSetSetting();
  // The sales rules are the main PC's; on a till they are read-only.
  const till = useTillLink();
  const [failure, setFailure] = useState<unknown>(null);
  const {
    control,
    register,
    handleSubmit,
    reset,
    formState: { errors, isSubmitting },
  } = useForm<RulesForm>({ resolver: zodResolver(rulesSchema), defaultValues: initial });
  const soldes = useFieldArray({ control, name: "soldes" });
  const stampMode = useWatch({ control, name: "stampMode" });
  const err = (message: string | undefined) => (message ? t(message) : undefined);

  const onSubmit = handleSubmit(async (values) => {
    setFailure(null);
    const next = toSettings(values);
    try {
      // Only the rules that differ from what the backend holds: each save is an audit row.
      for (const key of KEYS) {
        // an unset cut-off is the backend's default, 00:00 (Task 17's setting)
        const before = key === "sales.day_cutoff_hour" ? whole(stored[key], 0) : stored[key];
        if (JSON.stringify(next[key]) === JSON.stringify(before)) continue;
        await setSetting.mutateAsync({ key, value: next[key] });
      }
      reset(values);
      toast.success(t("settings:salesRules.saved"));
    } catch (error) {
      setFailure(error);
    }
  });

  const amountField = (name: "maxChange" | "varianceThreshold", id: string, label: string, hint: string) => (
    <Field id={id} label={label} hint={hint} error={err(errors[name]?.message)}>
      <Controller
        name={name}
        control={control}
        render={({ field }) => (
          <NumericInput
            id={id}
            aria-invalid={Boolean(errors[name])}
            value={field.value}
            onValueChange={field.onChange}
          />
        )}
      />
    </Field>
  );
  const check = (name: "chequeRequiresCustomer" | "blindClose", label: string) => (
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
    <form noValidate onSubmit={(event) => void onSubmit(event)} className="flex flex-col gap-6">
      <HostOnlyNote />
      <fieldset disabled={till.isTill} className="contents">
        <Card>
          <h2 className="text-xl font-bold">{t("settings:salesRules.payments")}</h2>
          <div className="grid gap-4 md:grid-cols-2">
            {amountField(
              "maxChange",
              "rules-max-change",
              t("settings:salesRules.maxChange"),
              t("settings:salesRules.maxChangeHint"),
            )}
            <Field id="rules-rounding" label={t("settings:salesRules.cashRounding")}>
              <NativeSelect id="rules-rounding" {...register("cashRounding")}>
                {ROUNDINGS.map((r) => (
                  <option key={r} value={r}>
                    {t(`settings:salesRules.rounding.${r}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            <Field id="rules-stamp" label={t("settings:salesRules.stampDuty")}>
              <NativeSelect id="rules-stamp" {...register("stampMode")}>
                {STAMP_MODES.map((m) => (
                  <option key={m} value={m}>
                    {t(`settings:salesRules.stampModes.${m}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          </div>
          {stampMode === "off" ? (
            <p role="note" className="rounded-control bg-warn-soft p-3 text-warn">
              {t("settings:salesRules.stampOffWarning")}
            </p>
          ) : null}
          {check("chequeRequiresCustomer", t("settings:salesRules.chequeRequiresCustomer"))}
        </Card>
        <Card>
          <h2 className="text-xl font-bold">{t("settings:salesRules.cash")}</h2>
          <div className="grid gap-4 md:grid-cols-2">
            {amountField(
              "varianceThreshold",
              "rules-variance",
              t("settings:salesRules.varianceThreshold"),
              t("settings:salesRules.varianceThresholdHint"),
            )}
          </div>
          {check("blindClose", t("settings:salesRules.blindClose"))}
          <div className="grid gap-4 md:grid-cols-2">
            <Field
              id="rules-day-cutoff"
              label={t("config:dayCutoff.label")}
              hint={t("config:dayCutoff.hint")}
            >
              <NativeSelect id="rules-day-cutoff" dir="ltr" {...register("dayCutoff")}>
                {DAY_CUTOFFS.map((hour) => (
                  <option key={hour} value={hour}>
                    {`${hour.padStart(2, "0")}:00`}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          </div>
        </Card>
        <Card>
          <h2 className="text-xl font-bold">{t("settings:salesRules.stock")}</h2>
          <div className="grid gap-4 md:grid-cols-3">
            <Field id="rules-negative" label={t("settings:salesRules.negativePolicy")}>
              <NativeSelect id="rules-negative" {...register("negativePolicy")}>
                {NEGATIVE_POLICIES.map((p) => (
                  <option key={p} value={p}>
                    {t(`settings:salesRules.negative.${p}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            <Field id="rules-expired" label={t("settings:salesRules.expiredLots")}>
              <NativeSelect id="rules-expired" {...register("expiredLots")}>
                {EXPIRED_LOTS.map((p) => (
                  <option key={p} value={p}>
                    {t(`settings:salesRules.expired.${p}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            <Field
              id="rules-expiry-days"
              label={t("settings:salesRules.expiryAlertDays")}
              error={err(errors.expiryAlertDays?.message)}
            >
              <Controller
                name="expiryAlertDays"
                control={control}
                render={({ field }) => (
                  <NumericInput
                    id="rules-expiry-days"
                    kind="integer"
                    maxLength={3}
                    aria-invalid={Boolean(errors.expiryAlertDays)}
                    value={field.value}
                    onValueChange={field.onChange}
                  />
                )}
              />
            </Field>
          </div>
        </Card>
        <Card>
          <h2 className="text-xl font-bold">{t("settings:salesRules.products")}</h2>
          <div className="grid gap-4 md:grid-cols-2">
            <Field
              id="rules-ean"
              label={t("settings:salesRules.eanPrefix")}
              hint={t("settings:salesRules.eanPrefixHint")}
              error={err(errors.eanPrefix?.message)}
            >
              <Controller
                name="eanPrefix"
                control={control}
                render={({ field }) => (
                  <NumericInput
                    id="rules-ean"
                    kind="integer"
                    maxLength={3}
                    aria-invalid={Boolean(errors.eanPrefix)}
                    value={field.value}
                    onValueChange={field.onChange}
                  />
                )}
              />
            </Field>
          </div>
        </Card>
        <Card>
          <h2 className="text-xl font-bold">{t("settings:salesRules.soldes")}</h2>
          <p className="text-sm text-ink-2">{t("settings:salesRules.soldesHint")}</p>
          {soldes.fields.length === 0 ? <p>{t("settings:salesRules.soldesNone")}</p> : null}
          {soldes.fields.map((item, index) => (
            <fieldset key={item.id} className="flex flex-wrap items-end gap-3 border-t border-line pt-4">
              <legend className="sr-only">{t("settings:salesRules.soldesWindow", { n: index + 1 })}</legend>
              <Controller
                name={`soldes.${index}`}
                control={control}
                render={({ field }) => (
                  <DateRangePicker
                    presets={[]}
                    value={field.value}
                    onChange={field.onChange}
                    error={err(errors.soldes?.[index]?.to?.message)}
                  />
                )}
              />
              {till.isTill ? null : (
                <Button
                  variant="ghost"
                  size="icon"
                  className="mb-0.5"
                  aria-label={t("settings:salesRules.soldesRemove", { n: index + 1 })}
                  title={t("settings:salesRules.soldesRemove", { n: index + 1 })}
                  onClick={() => soldes.remove(index)}
                >
                  <Trash2 aria-hidden />
                </Button>
              )}
            </fieldset>
          ))}
          {till.isTill ? null : (
            <div>
              <Button
                variant="outline"
                disabled={soldes.fields.length >= MAX_SOLDES_WINDOWS}
                onClick={() => soldes.append({ from: "", to: "" })}
              >
                <Plus aria-hidden />
                {t("settings:salesRules.soldesAdd")}
              </Button>
            </div>
          )}
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
            {t("settings:salesRules.save")}
          </Button>
        </div>
      )}
    </form>
  );
}
