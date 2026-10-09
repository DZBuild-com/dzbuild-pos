import { Copy } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { Splash } from "@/components/splash";
import { Button } from "@/components/ui/button";
import { formatAmountInput } from "@/lib/format";
import { useG50 } from "@/lib/ipc/fiscal";
import type { G50 } from "@/lib/ipc/types";
import { formatRate } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** A `YYYY-MM` month the backend can be asked for. */
export const MONTH = /^\d{4}-(0[1-9]|1[0-2])$/;

/**
 * The G50 of `month` as the backend computes it: one row per line with its box on the form and a
 * "Copier" button that puts the amount on the clipboard in the forms' input format (`1710,00`, no
 * grouping), the other rates, the totals and what stays outside the till. The screen adds nothing
 * up: every figure is the backend's.
 */
export function G50Helper({ month }: { month: string }) {
  const { t } = useTranslation("fiscal");
  const lang = useLang();
  const errorText = useErrorText();
  const g50 = useG50(MONTH.test(month) ? month : null);

  const copy = (amount: number) => {
    navigator.clipboard
      .writeText(formatAmountInput(amount, lang))
      .then(() => toast.success(t("g50.copied")))
      .catch((error: unknown) => toast.error(errorText(error)));
  };

  if (g50.isError) {
    return (
      <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
        {errorText(g50.error)}
      </p>
    );
  }
  if (!g50.data) return g50.isFetching ? <Splash inline /> : null;
  const data = g50.data;
  return (
    <section className="flex flex-col gap-4">
      <p className="text-sm text-ink-2">{t("g50.form", { form: data.form })}</p>
      <table className="w-full border-collapse">
        <thead className="border-b border-line text-sm text-ink-2">
          <tr>
            <th className="py-2 text-start font-semibold">{t("g50.line")}</th>
            <th className="py-2 text-start font-semibold">{t("g50.box")}</th>
            <th className="py-2 text-end font-semibold">{t("g50.amount")}</th>
            <th className="py-2">
              <span className="sr-only">{t("g50.copy")}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {data.lines.map((line) => {
            const label = t(`g50.lines.${line.key}`);
            return (
              <tr key={line.key} className="border-b border-line even:bg-surface-2">
                <td className="py-1.5 pe-3">{label}</td>
                <td className="py-1.5 pe-3">
                  <bdi dir="ltr">{line.box}</bdi>
                </td>
                <td className="py-1.5 text-end font-semibold">
                  <Amount value={line.amount} />
                </td>
                <td className="py-1.5 ps-3 text-end">
                  <Button
                    variant="ghost"
                    aria-label={t("g50.copyLine", { line: label })}
                    onClick={() => copy(line.amount)}
                  >
                    <Copy aria-hidden />
                    {t("g50.copy")}
                  </Button>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {data.otherRates.length > 0 ? <OtherRates g50={data} /> : null}
      <dl className="grid gap-x-6 gap-y-1 rounded-card border border-line bg-surface p-4 sm:grid-cols-[1fr_auto]">
        <dt>{t("g50.lines.tvaTotal")}</dt>
        <dd className="text-end font-semibold">
          <Amount value={data.tvaCollected} />
        </dd>
        <dt>{t("g50.lines.tvaDeductible")}</dt>
        <dd className="text-end font-semibold">
          <Amount value={data.tvaDeductible} />
        </dd>
        {data.tvaCredit > 0 ? (
          <>
            <dt className="font-bold">{t("g50.lines.tvaCredit")}</dt>
            <dd className="text-end text-xl font-bold">
              <Amount value={data.tvaCredit} />
            </dd>
          </>
        ) : (
          <>
            <dt className="font-bold">{t("g50.lines.tvaDue")}</dt>
            <dd className="text-end text-xl font-bold">
              <Amount value={data.tvaDue} />
            </dd>
          </>
        )}
        <dt>{t("g50.lines.stampDuty")}</dt>
        <dd className="text-end font-semibold">
          <Amount value={data.stampDuty} />
        </dd>
      </dl>
      <p className="rounded-control bg-info-soft p-3 text-info">{t("g50.outside")}</p>
    </section>
  );
}

/** Rates other than 0, 9 and 19 %: the form has no box for them, they are declared by hand. */
function OtherRates({ g50 }: { g50: G50 }) {
  const { t } = useTranslation("fiscal");
  const lang = useLang();
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-lg font-bold">{t("g50.otherRates")}</h2>
      <table className="w-full border-collapse">
        <thead className="border-b border-line text-sm text-ink-2">
          <tr>
            <th className="py-2 text-start font-semibold">{t("g50.rate")}</th>
            <th className="py-2 text-end font-semibold">{t("g50.baseHt")}</th>
            <th className="py-2 text-end font-semibold">{t("g50.tax")}</th>
          </tr>
        </thead>
        <tbody>
          {g50.otherRates.map((rate) => (
            <tr key={rate.rateBp} className="border-b border-line">
              <td className="py-1.5">
                <bdi dir="ltr">{formatRate(rate.rateBp, lang)}</bdi>
              </td>
              <td className="py-1.5 text-end">
                <Amount value={rate.baseHt} />
              </td>
              <td className="py-1.5 text-end">
                <Amount value={rate.tax} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
