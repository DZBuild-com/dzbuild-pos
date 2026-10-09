import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ProductPicker } from "@/features/purchases/purchase-lines";
import { formatPercentInput, parsePercentBp } from "@/lib/format";
import { LANGS, type Lang } from "@/lib/i18n";
import { useFlexyOperators } from "@/lib/ipc/phones";
import { useProduct } from "@/lib/ipc/products";
import { type SettingKey, useSetSetting, useSetting } from "@/lib/ipc/settings";
import type { FlexyOperator, JsonValue } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/**
 * The packs' settings the owner sets where they are used (M8 final review: they existed on the
 * Rust side only): a warn/block policy, a number of days, a trilingual text, a product, and the
 * flexy operators. Each saves through `settings_set` (`settings.company`, checked by the backend)
 * and says what went wrong.
 */
function useSave() {
  const save = useSetSetting();
  const errorText = useErrorText();
  const error = save.error ? (
    <p role="alert" className="text-danger">
      {errorText(save.error)}
    </p>
  ) : null;
  return {
    set: (key: SettingKey, value: JsonValue, onSuccess?: () => void) =>
      save.mutate({ key, value }, { onSuccess }),
    pending: save.isPending,
    saved: save.isSuccess,
    error,
  };
}

/** "Avertir" or "Refuser" (`warn` / `block`), saved when chosen. */
export function PolicySetting({
  settingKey,
  title,
  help,
  warn,
  block,
}: {
  settingKey: SettingKey;
  title: string;
  help: string;
  warn: string;
  block: string;
}) {
  const value = useSetting(settingKey).data;
  const { set, error } = useSave();
  const current = value === "block" ? "block" : "warn";
  return (
    <Card>
      <p className="text-ink-2">{help}</p>
      <RadioCards
        legend={title}
        name={settingKey}
        value={current}
        onChange={(next) => set(settingKey, next)}
        options={[
          { value: "warn", label: warn },
          { value: "block", label: block },
        ]}
      />
      {error}
    </Card>
  );
}

interface NumberProps {
  settingKey: SettingKey;
  id: string;
  title: string;
  label: string;
  help: string;
  min: number;
  max: number;
}

/** A whole number between `min` and `max`, saved with its button. */
export function NumberSetting(props: NumberProps) {
  const value = useSetting(props.settingKey).data;
  if (typeof value !== "number") return null;
  // the form starts from the stored value, again when it changes
  return <NumberForm key={value} initial={value} {...props} />;
}

function NumberForm({
  settingKey,
  id,
  title,
  label,
  help,
  min,
  max,
  initial,
}: NumberProps & { initial: number }) {
  const { t } = useTranslation();
  const { set, pending, error } = useSave();
  const [text, setText] = useState(String(initial));
  const n = Number(text);
  const valid = text.trim() !== "" && Number.isInteger(n) && n >= min && n <= max;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (valid) set(settingKey, n);
  };
  return (
    <Card>
      <h2 className="text-xl font-bold">{title}</h2>
      <form className="flex flex-wrap items-end gap-2" onSubmit={submit}>
        <div className="w-48">
          <Field
            id={id}
            label={label}
            hint={help}
            error={valid ? undefined : t("packs:settings.between", { min, max })}
          >
            <Input
              id={id}
              inputMode="numeric"
              dir="ltr"
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
          </Field>
        </div>
        <Button type="submit" variant="secondary" disabled={!valid || pending}>
          {t("common:actions.save")}
        </Button>
      </form>
      {error}
    </Card>
  );
}

type Texts = Record<Lang, string>;

interface TrilingualProps {
  settingKey: SettingKey;
  id: string;
  title: string;
  help: string;
  max: number;
}

function textsOf(value: unknown): Texts | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const v = value as Partial<Record<Lang, unknown>>;
  return {
    ar: typeof v.ar === "string" ? v.ar : "",
    fr: typeof v.fr === "string" ? v.fr : "",
    en: typeof v.en === "string" ? v.en : "",
  };
}

/** A text in Arabic, French and English (each customer gets it in their language), saved with
 * its button. */
export function TrilingualSetting(props: TrilingualProps) {
  const texts = textsOf(useSetting(props.settingKey).data);
  if (!texts) return null;
  return <TrilingualForm key={JSON.stringify(texts)} initial={texts} {...props} />;
}

function TrilingualForm({ settingKey, id, title, help, max, initial }: TrilingualProps & { initial: Texts }) {
  const { t } = useTranslation();
  const { set, pending, error, saved } = useSave();
  const [texts, setTexts] = useState<Texts>(initial);
  const valid = LANGS.every((l) => texts[l].trim().length > 0 && [...texts[l]].length <= max);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (valid) set(settingKey, { ar: texts.ar.trim(), fr: texts.fr.trim(), en: texts.en.trim() });
  };
  return (
    <Card>
      <h2 className="text-xl font-bold">{title}</h2>
      <p className="text-ink-2">{help}</p>
      <form className="flex flex-col gap-3" onSubmit={submit}>
        {LANGS.map((l) => (
          <Field key={l} id={`${id}-${l}`} label={t(`common:languages.${l}`)}>
            <textarea
              id={`${id}-${l}`}
              lang={l}
              dir={l === "ar" ? "rtl" : "ltr"}
              rows={3}
              maxLength={max}
              value={texts[l]}
              onChange={(e) => setTexts((all) => ({ ...all, [l]: e.target.value }))}
              className="w-full rounded-control border border-line bg-surface px-3 py-2 text-base text-ink"
            />
          </Field>
        ))}
        <div>
          <Button type="submit" variant="secondary" disabled={!valid || pending}>
            {t("common:actions.save")}
          </Button>
        </div>
        {saved ? (
          <p role="status" className="text-ok">
            {t("packs:settings.saved")}
          </p>
        ) : null}
      </form>
      {error}
    </Card>
  );
}

/** A product of the catalogue (or none), chosen with the purchases' picker. */
export function ProductSetting({
  settingKey,
  title,
  help,
  none,
  clear,
}: {
  settingKey: SettingKey;
  title: string;
  help: string;
  none: string;
  clear: string;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const value = useSetting(settingKey).data;
  const id = typeof value === "string" ? value : null;
  const product = useProduct(id);
  const { set, error } = useSave();
  return (
    <Card>
      <h2 className="text-xl font-bold">{title}</h2>
      <p className="text-ink-2">{help}</p>
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-semibold">
          {id === null ? none : product.data ? localizedName(product.data, lang) : t("common:status.loading")}
        </span>
        {id !== null ? (
          <Button variant="outline" onClick={() => set(settingKey, null)}>
            {clear}
          </Button>
        ) : null}
      </div>
      <ProductPicker onPick={(picked) => set(settingKey, picked.productId)} />
      {error}
    </Card>
  );
}

interface OperatorDraft {
  code: string;
  productId: string | null;
  commission: string;
}

/** VRT-002: each operator's top-up product and commission (`flexy.operators`). An operator
 * without its product sells nothing; the commission is what the flexy sales report counts. */
export function FlexyOperatorsSetting() {
  const operators = useFlexyOperators();
  if (!operators.data) return null;
  return (
    <OperatorsForm
      key={JSON.stringify(operators.data.map((o) => [o.code, o.productId, o.commissionBp]))}
      operators={operators.data}
      onSaved={() => void operators.refetch()}
    />
  );
}

function OperatorsForm({ operators, onSaved }: { operators: FlexyOperator[]; onSaved: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const { set, pending, error, saved } = useSave();
  const [drafts, setDrafts] = useState<OperatorDraft[]>(() =>
    operators.map((o) => ({
      code: o.code,
      productId: o.productId,
      commission: formatPercentInput(o.commissionBp, lang),
    })),
  );
  const [picking, setPicking] = useState<string | null>(null);
  const bps = drafts.map((d) => parsePercentBp(d.commission));
  const valid = bps.every((bp) => bp !== null && bp >= 0 && bp <= 2_000);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!valid) return;
    set(
      "flexy.operators",
      drafts.flatMap((d, i) =>
        d.productId ? [{ code: d.code, productId: d.productId, commissionBp: bps[i] ?? 0 }] : [],
      ),
      onSaved,
    );
  };
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("packs:settings.flexy.title")}</h2>
      <p className="text-ink-2">{t("packs:settings.flexy.help")}</p>
      <form className="flex flex-col gap-3" onSubmit={submit}>
        {operators.map((operator, index) => {
          const draft = drafts[index];
          if (!draft) return null;
          const bp = bps[index];
          return (
            <div
              key={operator.code}
              className="flex flex-wrap items-end gap-3 rounded-control border border-line p-3"
            >
              <span className="min-w-32 font-semibold">{localizedName(operator, lang)}</span>
              <div className="w-36">
                <Field
                  id={`flexy-commission-${operator.code}`}
                  label={t("packs:settings.flexy.commission")}
                  error={
                    bp === null || bp === undefined || bp > 2_000
                      ? t("packs:settings.flexy.commissionInvalid")
                      : undefined
                  }
                >
                  <Input
                    id={`flexy-commission-${operator.code}`}
                    inputMode="decimal"
                    dir="ltr"
                    value={draft.commission}
                    onChange={(e) =>
                      setDrafts((all) =>
                        all.map((d, i) => (i === index ? { ...d, commission: e.target.value } : d)),
                      )
                    }
                  />
                </Field>
              </div>
              <OperatorProduct
                productId={draft.productId}
                picking={picking === operator.code}
                onPick={() => setPicking(picking === operator.code ? null : operator.code)}
                onPicked={(productId) => {
                  setDrafts((all) => all.map((d, i) => (i === index ? { ...d, productId } : d)));
                  setPicking(null);
                }}
              />
            </div>
          );
        })}
        <div>
          <Button type="submit" variant="primary" disabled={!valid || pending}>
            {t("common:actions.save")}
          </Button>
        </div>
        {saved ? (
          <p role="status" className="text-ok">
            {t("packs:settings.saved")}
          </p>
        ) : null}
      </form>
      {error}
    </Card>
  );
}

function OperatorProduct({
  productId,
  picking,
  onPick,
  onPicked,
}: {
  productId: string | null;
  picking: boolean;
  onPick: () => void;
  onPicked: (productId: string) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const product = useProduct(productId);
  return (
    <div className="flex flex-col gap-2">
      <span className="text-sm text-ink-2">{t("packs:settings.flexy.product")}</span>
      <span className="flex flex-wrap items-center gap-2">
        <span className="font-semibold">
          {productId === null
            ? t("packs:settings.flexy.noProduct")
            : product.data
              ? localizedName(product.data, lang)
              : t("common:status.loading")}
        </span>
        <Button type="button" variant="outline" onClick={onPick}>
          {t("packs:settings.flexy.change")}
        </Button>
      </span>
      {picking ? <ProductPicker autoFocus onPick={(p) => onPicked(p.productId)} /> : null}
    </div>
  );
}
