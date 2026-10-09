import { type FormEvent, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { PinInput } from "@/components/pin-pad";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { JoinWizard } from "@/features/network/JoinWizard";
import { SCALE_ACTIVITIES } from "@/features/packs/modules";
import { markUrl } from "@/lib/brand";
import { toAsciiDigits } from "@/lib/digits";
import { LANGS, currentLang, setLanguage } from "@/lib/i18n";
import { useDatapack, useOpenLink } from "@/lib/ipc/app";
import { useHardwareConfig, useSaveHardware } from "@/lib/ipc/hardware";
import { useCompleteFirstRun, useFinishSetup } from "@/lib/ipc/setup";
import type {
  HardwareConfig,
  PriceMode,
  PrinterConfig,
  ScaleConfig,
  ShopActivity,
  TaxRegime,
} from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { pinErrors } from "@/lib/pin";
import { useErrorText, useFieldError } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { FirstScreenOptions } from "./FirstScreenOptions";
import { initialSetupLang, rememberSetupLang } from "./setup-lang";
import { PrinterStep } from "./PrinterStep";
import { ScaleStep } from "./ScaleStep";
import { TemplateStep } from "./TemplateStep";

const STEP_KEYS = [
  "setup:steps.language",
  "setup:steps.shop",
  "setup:steps.owner",
  "setup:steps.printer",
] as const;

/** The shop and its owner exist from this step on: there is no way back. */
const PRINTER_STEP = 3;
/** ONB-009: after the printer, for the activities that weigh (`SCALE_ACTIVITIES`). */
const SCALE_STEP = 4;

export interface ShopDraft {
  shopName: string;
  activity: ShopActivity;
  taxRegime: TaxRegime;
  priceMode: PriceMode;
  wilaya: string;
  phone: string;
}

export interface OwnerDraft {
  ownerName: string;
  pin: string;
  pinConfirm: string;
}

type Errors = Partial<Record<string, string>>;

export function validateShop(shop: ShopDraft): Errors {
  const name = shop.shopName.trim();
  if (!name) return { shopName: "validation:required" };
  if (name.length > 80) return { shopName: "validation:tooLong" };
  return {};
}

export function validateOwner(owner: OwnerDraft): Errors {
  const errors: Errors = { ...pinErrors(owner.pin, owner.pinConfirm) };
  if (!owner.ownerName.trim()) errors.ownerName = "validation:required";
  return errors;
}

export function SetupWizard() {
  const { t } = useTranslation();
  const lang = useLang();
  // The language chosen on this PC before (leaving the demo, a restart before the setup ends),
  // else Windows' own when it is Arabic or English.
  useEffect(() => {
    const wanted = initialSetupLang();
    if (wanted !== currentLang()) void setLanguage(wanted);
  }, []);
  const errorText = useErrorText();
  const wilayas = useDatapack().data?.wilayas ?? [];
  const complete = useCompleteFirstRun();
  const finish = useFinishSetup();
  const saveHardware = useSaveHardware();
  const openLink = useOpenLink();
  const [step, setStep] = useState(0);
  // A new shop, or a till of a shop whose main PC already runs on this network (LAN-001).
  const [mode, setMode] = useState<"new" | "join">("new");
  const [joining, setJoining] = useState(false);
  // The shop and its owner exist from the printer step on: hardware_get needs the owner signed in.
  const hardware = useHardwareConfig(step >= PRINTER_STEP);
  const [printer, setPrinter] = useState<PrinterConfig | null>(null);
  // The scale chosen in the scale step; `undefined` until the owner touches it.
  const [scale, setScale] = useState<ScaleConfig | null | undefined>(undefined);
  const [shop, setShop] = useState<ShopDraft>({
    shopName: "",
    activity: "general",
    taxRegime: "ifu",
    priceMode: "ttc",
    wilaya: "",
    phone: "",
  });
  const [owner, setOwner] = useState<OwnerDraft>({ ownerName: "", pin: "", pinConfirm: "" });
  const [errors, setErrors] = useState<Errors>({});
  const steps: readonly string[] = SCALE_ACTIVITIES.includes(shop.activity)
    ? [...STEP_KEYS, "setup:steps.scale"]
    : STEP_KEYS;
  const last = steps.length - 1;
  const errorOf = useFieldError(errors);
  const scaleDraft: HardwareConfig = {
    ...(hardware.data ?? {}),
    scale: scale === undefined ? (hardware.data?.scale ?? null) : scale,
  };

  const goBack = () => {
    setErrors({});
    setStep((current) => Math.max(0, current - 1));
  };
  /** Saves the printer and the scale the owner chose (if any), then opens the app. */
  const saveAndFinish = (withScale: boolean) => {
    const scaleChosen = withScale && scale !== undefined;
    if (!printer && !scaleChosen) {
      finish();
      return;
    }
    // hardware.json belongs to the PC and may outlive a reinstall: keep what it already holds.
    const current = hardware.data ?? {};
    saveHardware.mutate(
      {
        ...current,
        ...(printer
          ? {
              printers: [...(current.printers ?? []).filter((p) => p.id !== printer.id), printer],
              receiptPrinterId: printer.id,
            }
          : {}),
        ...(scaleChosen ? { scale } : {}),
      },
      { onSuccess: finish },
    );
  };
  /** "Passer": the printer step goes on to the scale without a printer; the last step ends. */
  const skip = () => {
    if (step === PRINTER_STEP && step < last) {
      setPrinter(null);
      setStep(SCALE_STEP);
    } else if (step === PRINTER_STEP) finish();
    else saveAndFinish(false);
  };
  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    if (step === 0 && mode === "join") {
      setJoining(true);
      return;
    }
    if (step === last) {
      saveAndFinish(true);
      return;
    }
    if (step === PRINTER_STEP) {
      setStep(SCALE_STEP);
      return;
    }
    const found = step === 1 ? validateShop(shop) : step === 2 ? validateOwner(owner) : {};
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    if (step === 2) {
      const phone = toAsciiDigits(shop.phone.trim());
      complete.mutate(
        {
          lang,
          shopName: shop.shopName.trim(),
          activity: shop.activity,
          taxRegime: shop.taxRegime,
          priceMode: shop.priceMode,
          wilaya: shop.wilaya === "" ? null : shop.wilaya,
          phone: phone === "" ? null : phone,
          ownerName: owner.ownerName.trim(),
          ownerPin: owner.pin,
        },
        { onSuccess: () => setStep(PRINTER_STEP) },
      );
      return;
    }
    setStep((current) => current + 1);
  };

  if (joining) {
    return (
      <div className="min-h-dvh bg-bg px-4 pt-8 pb-[calc(2rem+var(--osk-inset,0px))] text-ink">
        <div className="mx-auto flex max-w-2xl flex-col gap-6">
          <header className="flex items-center gap-3">
            <img src={markUrl} alt="" className="size-12" />
            <h1 className="text-3xl font-bold">{t("setup:title")}</h1>
          </header>
          <Card>
            <JoinWizard onBack={() => setJoining(false)} />
          </Card>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-dvh bg-bg px-4 pt-8 pb-[calc(2rem+var(--osk-inset,0px))] text-ink">
      <form
        noValidate
        onSubmit={onSubmit}
        onKeyDown={(event) => {
          // the shop exists from the printer step on: there is no way back
          if (event.key === "Escape" && step > 0 && step < PRINTER_STEP) goBack();
        }}
        className="mx-auto flex max-w-2xl flex-col gap-6"
      >
        <header className="flex items-center gap-3">
          <img src={markUrl} alt="" className="size-12" />
          <h1 className="text-3xl font-bold">{t("setup:title")}</h1>
        </header>

        <div className="flex flex-col gap-2">
          <div
            role="progressbar"
            aria-valuemin={1}
            aria-valuemax={steps.length}
            aria-valuenow={step + 1}
            aria-label={t("setup:progress", { step: step + 1, total: steps.length })}
            className="h-2 overflow-hidden rounded-full bg-surface-2"
          >
            <div
              className="h-full bg-brand transition-all"
              style={{ inlineSize: `${((step + 1) / steps.length) * 100}%` }}
            />
          </div>
          <ol className="flex justify-between text-sm text-ink-2">
            {steps.map((key, index) => (
              <li
                key={key}
                aria-current={index === step ? "step" : undefined}
                className={cn(index === step && "font-bold text-ink")}
              >
                {t(key)}
              </li>
            ))}
          </ol>
        </div>

        <Card>
          {step === 0 ? (
            <fieldset className="flex flex-col gap-4">
              <legend className="text-2xl font-bold">{t("setup:language.title")}</legend>
              <p className="text-ink-2">{t("setup:language.hint")}</p>
              <div className="grid gap-3 sm:grid-cols-3">
                {LANGS.map((option) => (
                  <Button
                    key={option}
                    size="lg"
                    variant="outline"
                    lang={option}
                    aria-pressed={lang === option}
                    className={cn(lang === option && "border-brand bg-brand-soft")}
                    onClick={() => {
                      rememberSetupLang(option);
                      void setLanguage(option);
                    }}
                  >
                    {t(`common:languages.${option}`)}
                  </Button>
                ))}
              </div>
              <RadioCards
                legend={t("sync:lan.setup.choiceTitle")}
                name="pc-mode"
                value={mode}
                onChange={setMode}
                options={[
                  { value: "new", label: t("sync:lan.setup.newShop"), hint: t("sync:lan.setup.newShopHelp") },
                  {
                    value: "join",
                    label: t("sync:lan.setup.joinShop"),
                    hint: t("sync:lan.setup.joinShopHelp"),
                  },
                ]}
              />
            </fieldset>
          ) : null}

          {step === 1 ? (
            <fieldset className="flex flex-col gap-4">
              <legend className="text-2xl font-bold">{t("setup:shop.title")}</legend>
              <Field id="shop-name" label={t("setup:shop.name")} error={errorOf("shopName")}>
                <Input
                  id="shop-name"
                  autoFocus
                  dir="auto"
                  aria-invalid={Boolean(errors.shopName)}
                  value={shop.shopName}
                  onChange={(event) => setShop({ ...shop, shopName: event.target.value })}
                />
              </Field>
              <TemplateStep value={shop.activity} onChange={(activity) => setShop({ ...shop, activity })} />
              <RadioCards
                legend={t("setup:shop.regime")}
                name="taxRegime"
                value={shop.taxRegime}
                onChange={(taxRegime) => setShop({ ...shop, taxRegime })}
                options={[
                  { value: "ifu", label: t("setup:shop.regimeIfu"), hint: t("setup:shop.regimeIfuHint") },
                  { value: "reel", label: t("setup:shop.regimeReel"), hint: t("setup:shop.regimeReelHint") },
                ]}
              />
              <RadioCards
                legend={t("setup:shop.priceMode")}
                name="priceMode"
                value={shop.priceMode}
                onChange={(priceMode) => setShop({ ...shop, priceMode })}
                options={[
                  { value: "ttc", label: t("setup:shop.priceTtc") },
                  { value: "ht", label: t("setup:shop.priceHt") },
                ]}
              />
              <div className="grid gap-4 sm:grid-cols-2">
                <Field id="shop-wilaya" label={t("setup:shop.wilaya")} optional={t("setup:shop.optional")}>
                  <NativeSelect
                    id="shop-wilaya"
                    value={shop.wilaya}
                    onChange={(event) => setShop({ ...shop, wilaya: event.target.value })}
                  >
                    <option value="">{t("setup:shop.wilayaNone")}</option>
                    {wilayas.map((w) => (
                      <option key={w.code} value={w.code}>{`${w.code} ${localizedName(w, lang)}`}</option>
                    ))}
                  </NativeSelect>
                </Field>
                <Field id="shop-phone" label={t("setup:shop.phone")} optional={t("setup:shop.optional")}>
                  <Input
                    id="shop-phone"
                    type="tel"
                    inputMode="tel"
                    dir="ltr"
                    value={shop.phone}
                    onChange={(event) => setShop({ ...shop, phone: event.target.value })}
                  />
                </Field>
              </div>
            </fieldset>
          ) : null}

          {step === 2 ? (
            <fieldset className="flex flex-col gap-4">
              <legend className="text-2xl font-bold">{t("setup:owner.title")}</legend>
              <p className="text-ink-2">{t("setup:owner.hint")}</p>
              <Field id="owner-name" label={t("setup:owner.name")} error={errorOf("ownerName")}>
                <Input
                  id="owner-name"
                  autoFocus
                  dir="auto"
                  value={owner.ownerName}
                  onChange={(event) => setOwner({ ...owner, ownerName: event.target.value })}
                />
              </Field>
              <div className="grid gap-4 sm:grid-cols-2">
                <Field id="owner-pin" label={t("setup:owner.pin")} error={errorOf("pin")}>
                  <PinInput
                    id="owner-pin"
                    value={owner.pin}
                    onChange={(pin) => setOwner({ ...owner, pin })}
                  />
                </Field>
                <Field
                  id="owner-pin-confirm"
                  label={t("setup:owner.pinConfirm")}
                  error={errorOf("pinConfirm")}
                >
                  <PinInput
                    id="owner-pin-confirm"
                    value={owner.pinConfirm}
                    onChange={(pinConfirm) => setOwner({ ...owner, pinConfirm })}
                  />
                </Field>
              </div>
            </fieldset>
          ) : null}

          {step === PRINTER_STEP ? <PrinterStep printer={printer} onChange={setPrinter} /> : null}
          {step === SCALE_STEP ? (
            <ScaleStep config={scaleDraft} onChange={(next) => setScale(next.scale ?? null)} />
          ) : null}
        </Card>

        {/* The free edition, said once on the last step: one quiet line, no extra step. */}
        {step === last ? (
          <p className="text-sm text-ink-2">
            {t("setup:free.note")}{" "}
            <button
              type="button"
              className="inline-flex min-h-11 items-center font-semibold text-info underline focus-visible:outline-3 focus-visible:outline-focus"
              onClick={() => openLink.mutate("dzbuild_register")}
            >
              <bdi dir="ltr">{t("setup:free.link")}</bdi>
            </button>
          </p>
        ) : null}

        {complete.error ? (
          <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
            {errorText(complete.error)}
          </p>
        ) : null}
        {saveHardware.error ? (
          <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
            {errorText(saveHardware.error)}
          </p>
        ) : null}

        {/* Finding 22: the way on is always in view on a 1024x768 screen (sticky), before the
            other first-screen choices. */}
        <div className="sticky bottom-[var(--osk-inset,0px)] z-10 flex justify-end gap-2 bg-bg py-2">
          {step > 0 && step < PRINTER_STEP ? (
            <Button
              variant="outline"
              onClick={goBack}
              shortcut="Escape"
              shortcutLabel={t("common:keys.escape")}
            >
              {t("common:actions.back")}
            </Button>
          ) : null}
          {step >= PRINTER_STEP ? (
            <Button variant="ghost" disabled={saveHardware.isPending} onClick={skip}>
              {step === SCALE_STEP
                ? t("packs:scale.skip")
                : step < last
                  ? t("setup:skip")
                  : t("setup:printer.skip")}
            </Button>
          ) : null}
          <Button
            type="submit"
            variant="primary"
            // a device is saved next to what hardware.json already holds: wait until it is read
            disabled={
              complete.isPending ||
              saveHardware.isPending ||
              ((printer !== null || scale !== undefined) && hardware.isLoading)
            }
          >
            {step === last ? t("setup:finish") : t("common:actions.next")}
          </Button>
        </div>

        {step === 0 ? <FirstScreenOptions mode={mode} /> : null}
      </form>
    </div>
  );
}
