import { Printer, Scale } from "lucide-react";
import { useMemo, useState } from "react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { EmptyState } from "@/components/empty-state";
import { LanguageToggle } from "@/components/language-toggle";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { dayToMs, isDay, today } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import { toAppError } from "@/lib/ipc/client";
import { useTillLink } from "@/lib/ipc/lan";
import { usePrintZakat, useSaveZakat, useZakatPreview, useZakatStatements } from "@/lib/ipc/staff";
import type { Valuation, ZakatInput, ZakatPreview } from "@/lib/ipc/types";
import { formatRate, parseAmount } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { NisabCard } from "./NisabCard";

const VALUATIONS: readonly Valuation[] = ["cost", "sale_price"];

interface ZakatForm {
  day: string;
  valuation: Valuation;
  bank: string;
  otherAssets: string;
  otherDebts: string;
  note: string;
}

type AmountField = "bank" | "otherAssets" | "otherDebts";
const AMOUNT_FIELDS: readonly AmountField[] = ["bank", "otherAssets", "otherDebts"];

/** An optional amount typed in DA: empty is zero; `null` when it cannot be read. */
function amountOrZero(text: string): number | null {
  if (text.trim() === "") return 0;
  const value = parseAmount(text);
  return value !== null && value >= 0 ? value : null;
}

/** What the backend computes from the form, or `null` while a field cannot be read. */
function toInput(v: ZakatForm): ZakatInput | null {
  const bank = amountOrZero(v.bank);
  const otherAssets = amountOrZero(v.otherAssets);
  const otherDebts = amountOrZero(v.otherDebts);
  if (bank === null || otherAssets === null || otherDebts === null) return null;
  if (v.day !== "" && !isDay(v.day)) return null;
  return {
    day: v.day === "" ? null : v.day,
    valuation: v.valuation,
    bank,
    otherAssets,
    otherDebts,
    note: v.note.trim() === "" ? null : v.note.trim(),
  };
}

/**
 * `/zakat` (FIN-007): the nisab of the year the shop enters itself (the software never supplies
 * it); the day with its Hijri date, how the stock is valued, what the books do not hold (bank,
 * other assets, other debts); the live statement from the backend (every line, the base against
 * the shop's nisab, the rate, the zakat due), never a fatwa, or what to do first while no nisab
 * is entered; then the saved statements (`ZK…`), each printable in any of the three languages.
 */
export function ZakatScreen() {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const till = useTillLink();
  const save = useSaveZakat();
  const { control, register, handleSubmit } = useForm<ZakatForm>({
    defaultValues: { day: today(), valuation: "cost", bank: "", otherAssets: "", otherDebts: "", note: "" },
  });
  const values = useWatch({ control }) as ZakatForm;
  const { day, valuation, bank, otherAssets, otherDebts, note } = values;
  const input = useMemo(
    () => toInput({ day, valuation, bank, otherAssets, otherDebts, note }),
    [day, valuation, bank, otherAssets, otherDebts, note],
  );
  const asked = useDebounced(input, 300);
  const preview = useZakatPreview(asked);
  const invalid = (field: AmountField) => amountOrZero(values[field]) === null;

  const onSubmit = handleSubmit((v) => {
    const statement = toInput(v);
    if (!statement) return;
    save.mutate(statement, {
      onSuccess: (saved) => toast.success(t("zakat:recorded", { number: saved.number })),
    });
  });

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        <h1 className="text-3xl font-bold">{t("zakat:title")}</h1>
        <p className="text-ink-2">{t("zakat:intro")}</p>
      </div>
      {till.isTill ? (
        <p role="note" className="rounded-control bg-info-soft p-3 text-info">
          {t("zakat:mainPcOnly")}
        </p>
      ) : null}
      <NisabCard />
      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <h2 className="text-xl font-bold">{t("zakat:inputs")}</h2>
          <form noValidate onSubmit={(event) => void onSubmit(event)} className="flex flex-col gap-4">
            <div className="grid gap-4 sm:grid-cols-2">
              <Field
                id="zakat-day"
                label={t("zakat:day")}
                error={day !== "" && !isDay(day) ? t("validation:required") : undefined}
              >
                <Input id="zakat-day" type="date" dir="ltr" {...register("day")} />
              </Field>
              <Field id="zakat-valuation" label={t("zakat:valuation.label")}>
                <NativeSelect id="zakat-valuation" {...register("valuation")}>
                  {VALUATIONS.map((v) => (
                    <option key={v} value={v}>
                      {t(`zakat:valuation.${v}`)}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
            </div>
            <div className="grid gap-4 sm:grid-cols-3">
              {AMOUNT_FIELDS.map((field) => (
                <Field
                  key={field}
                  id={`zakat-${field}`}
                  label={t(`zakat:${field}`)}
                  optional={t("customers:form.optional")}
                  error={invalid(field) ? t("validation:amountFormat") : undefined}
                >
                  <Controller
                    name={field}
                    control={control}
                    render={({ field: f }) => (
                      <NumericInput
                        id={`zakat-${field}`}
                        aria-invalid={invalid(field)}
                        value={f.value}
                        onValueChange={f.onChange}
                      />
                    )}
                  />
                </Field>
              ))}
            </div>
            <Field id="zakat-note" label={t("zakat:note")} optional={t("customers:form.optional")}>
              <Input id="zakat-note" dir="auto" maxLength={200} {...register("note")} />
            </Field>
            {save.error ? (
              <p role="alert" className="text-danger">
                {errorText(save.error)}
              </p>
            ) : null}
            <Button
              type="submit"
              variant="primary"
              className="self-start"
              disabled={input === null || save.isPending}
            >
              {t("zakat:save")}
            </Button>
          </form>
        </Card>
        <Card aria-labelledby="zakat-preview-title">
          <h2 id="zakat-preview-title" className="text-xl font-bold">
            {t("zakat:preview")}
          </h2>
          {preview.error && toAppError(preview.error).code === "zakat_nisab_missing" ? (
            <EmptyState
              icon={Scale}
              title={t("zakat:nisabMissing.title")}
              body={t("zakat:nisabMissing.body")}
            />
          ) : preview.error ? (
            <p role="alert" className="text-danger">
              {errorText(preview.error)}
            </p>
          ) : preview.data ? (
            <Statement preview={preview.data} />
          ) : (
            <p className="text-ink-2">{t("zakat:computing")}</p>
          )}
          <p className="rounded-card border border-info bg-info-soft p-4">{t("zakat:disclaimer")}</p>
        </Card>
      </div>
      <SavedStatements />
    </div>
  );
}

/** The backend's figures: the seven lines, the base, the nisab the shop entered with its note on
 * the source, the rate and the zakat due (or the below-nisab sentence). */
function Statement({ preview }: { preview: ZakatPreview }) {
  const { t } = useTranslation();
  const lang = useLang();
  // `null` only on a statement saved before the shop entered its own nisab (never a preview)
  const note = preview.nisabSource?.trim();
  const source = note === undefined ? null : note === "" ? t("zakat:nisabByShop") : note;
  const lines: Array<[string, number]> = [
    [t("zakat:lines.stock", { valuation: t(`zakat:valuation.${preview.valuation}`) }), preview.stockValue],
    [t("zakat:lines.cash"), preview.cash],
    [t("zakat:lines.bank"), preview.bank],
    [t("zakat:lines.otherAssets"), preview.otherAssets],
    [t("zakat:lines.receivables"), preview.receivables],
    [t("zakat:lines.payables"), preview.payables],
    [t("zakat:lines.otherDebts"), preview.otherDebts],
  ];
  return (
    <div className="flex flex-col gap-4">
      <dl className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-1">
        <dt className="text-ink-2">{t("zakat:day")}</dt>
        <dd className="text-end">
          <bdi dir="ltr">{formatDate(dayToMs(preview.day), lang)}</bdi>
        </dd>
        <dt className="text-ink-2">{t("zakat:hijri")}</dt>
        <dd className="text-end">{preview.hijriLabel}</dd>
      </dl>
      <dl className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-1 border-t border-line pt-3">
        {lines.map(([label, value]) => (
          <div key={label} className="contents">
            <dt>{label}</dt>
            <dd className="text-end">
              <Amount value={value} />
            </dd>
          </div>
        ))}
      </dl>
      <dl className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-1 border-t border-line pt-3">
        <dt className="font-bold">{t("zakat:base")}</dt>
        <dd className="text-end">
          <Amount value={preview.base} className="font-bold" />
        </dd>
        <dt>{t("zakat:nisab")}</dt>
        <dd className="text-end">
          <Amount value={preview.nisab} />
        </dd>
        {source ? (
          <dd className="col-span-2 text-sm text-ink-2">{t("zakat:nisabSource", { source })}</dd>
        ) : null}
        <dt>{t("zakat:rate")}</dt>
        <dd className="text-end">
          <bdi dir="ltr" className="tabular-nums">
            {formatRate(preview.rateBp, lang)}
          </bdi>
        </dd>
      </dl>
      {preview.belowNisab ? (
        <p role="status" className="rounded-control bg-surface-2 p-3 font-semibold">
          {t("zakat:belowNisab")}
        </p>
      ) : (
        <dl className="flex flex-wrap items-baseline justify-between gap-3 rounded-control bg-brand-soft p-3">
          <dt className="text-xl font-bold">{t("zakat:due")}</dt>
          <dd>
            <Amount value={preview.zakatDue} className="text-[2rem] leading-tight font-bold" />
          </dd>
        </dl>
      )}
    </div>
  );
}

/** The saved statements, newest first (kept on the main PC), each printed on this PC's A4
 * printer in the chosen language. */
function SavedStatements() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const statements = useZakatStatements();
  const print = usePrintZakat();
  const [printLang, setPrintLang] = useState<Lang>(lang);
  const rows = statements.data ?? [];
  return (
    <Card>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-xl font-bold">{t("zakat:saved")}</h2>
        {rows.length > 0 ? (
          <div className="flex flex-wrap items-center gap-3">
            <span className="font-semibold">{t("billing:print.language")}</span>
            <LanguageToggle value={printLang} onChange={setPrintLang} />
          </div>
        ) : null}
      </div>
      {statements.error ? (
        <p role="alert" className="text-danger">
          {errorText(statements.error)}
        </p>
      ) : rows.length === 0 ? (
        <p className="text-ink-2">
          {statements.isPending ? t("common:status.loading") : t("zakat:savedEmpty")}
        </p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse">
            <thead className="text-sm text-ink-2">
              <tr className="h-10">
                <th scope="col" className="text-start font-semibold">
                  {t("zakat:columns.number")}
                </th>
                <th scope="col" className="text-start font-semibold">
                  {t("zakat:columns.day")}
                </th>
                <th scope="col" className="text-end font-semibold">
                  {t("zakat:columns.base")}
                </th>
                <th scope="col" className="text-end font-semibold">
                  {t("zakat:columns.due")}
                </th>
                <th scope="col" className="text-start font-semibold">
                  {t("zakat:columns.by")}
                </th>
                <th scope="col">
                  <span className="sr-only">{t("zakat:print")}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((s) => (
                <tr key={s.id} className="h-12 border-t border-line even:bg-surface-2">
                  <td>
                    <bdi dir="ltr">{s.number}</bdi>
                  </td>
                  <td>
                    <bdi dir="ltr">{formatDate(dayToMs(s.preview.day), lang)}</bdi>{" "}
                    <span className="text-ink-2">{s.preview.hijriLabel}</span>
                  </td>
                  <td className="text-end">
                    <Amount value={s.preview.base} />
                  </td>
                  <td className="text-end">
                    <Amount value={s.preview.zakatDue} className="font-bold" />
                  </td>
                  <td>{s.userName}</td>
                  <td className="text-end">
                    <Button
                      variant="ghost"
                      disabled={print.isPending}
                      aria-label={`${t("zakat:print")} ${s.number}`}
                      onClick={() =>
                        print.mutate(
                          { id: s.id, lang: printLang },
                          {
                            onSuccess: () => toast.success(t("zakat:printed")),
                            onError: (error) => toast.error(errorText(error)),
                          },
                        )
                      }
                    >
                      <Printer aria-hidden />
                      {t("zakat:print")}
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}
