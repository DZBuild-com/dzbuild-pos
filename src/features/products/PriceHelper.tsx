import { TriangleAlert } from "lucide-react";
import { useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { parsePercentBp } from "@/lib/format";
import { usePriceHelp } from "@/lib/ipc/product-extras";
import type { PriceHelpInput } from "@/lib/ipc/types";
import { formatRate } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/**
 * PRC-008 (PRD-060/061): the margin of the typed sale price over the cost, and the sale price that
 * gives a target margin, both from `products_price_help` (the UI never divides money). The price
 * is in the company's price mode (TTC or HT); `onUsePrice` puts the suggested price in the form.
 */
export function PriceHelper({
  cost,
  price,
  taxRateId,
  onUsePrice,
}: {
  /** HT cost in centimes (the CMUP or the typed purchase price); null while unknown. */
  cost: number | null;
  price: number | null;
  taxRateId: string;
  onUsePrice?: (price: number) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const id = useId();
  const [target, setTarget] = useState("");
  const targetBp = target.trim() === "" ? null : parsePercentBp(target);
  // Built from its primitives so the debounced value only changes with them.
  const input = useMemo<PriceHelpInput | null>(
    () => (cost === null || taxRateId === "" ? null : { cost, price, targetMarginBp: targetBp, taxRateId }),
    [cost, price, targetBp, taxRateId],
  );
  const help = usePriceHelp(useDebounced(input, 250));
  const data = help.data;
  const suggested = data?.priceForTarget ?? null;
  return (
    <section
      role="group"
      aria-labelledby={`${id}-title`}
      className="flex flex-col gap-3 rounded-card border border-line bg-surface-2 p-4"
    >
      <h3 id={`${id}-title`} className="font-semibold">
        {t("products:priceHelp.title")}
      </h3>
      {data && data.margin !== null ? (
        <dl className="grid gap-x-6 gap-y-1 sm:grid-cols-3">
          <div>
            <dt className="text-sm text-ink-2">{t("products:priceHelp.margin")}</dt>
            <dd className="font-semibold">
              <Amount value={data.margin} />
            </dd>
          </div>
          {data.marginBp !== null ? (
            <div>
              <dt className="text-sm text-ink-2">{t("products:priceHelp.marginRate")}</dt>
              <dd className="font-semibold tabular-nums">
                <bdi dir="ltr">{formatRate(data.marginBp, lang)}</bdi>
              </dd>
            </div>
          ) : null}
          {data.markupBp !== null ? (
            <div>
              <dt className="text-sm text-ink-2">{t("products:priceHelp.markupRate")}</dt>
              <dd className="font-semibold tabular-nums">
                <bdi dir="ltr">{formatRate(data.markupBp, lang)}</bdi>
              </dd>
            </div>
          ) : null}
        </dl>
      ) : (
        <p className="text-sm text-ink-2">{t("products:priceHelp.noPrice")}</p>
      )}
      {data?.belowCost ? (
        <p role="alert" className="flex items-center gap-2 font-semibold text-warn">
          <TriangleAlert aria-hidden className="size-5 shrink-0" />
          {t("products:priceHelp.belowCost")}
        </p>
      ) : null}
      <div className="flex flex-wrap items-end gap-3">
        <Field
          id={`${id}-target`}
          label={t("products:priceHelp.target")}
          error={target.trim() !== "" && targetBp === null ? t("validation:percentRange") : undefined}
        >
          <NumericInput id={`${id}-target`} className="w-32" value={target} onValueChange={setTarget} />
        </Field>
        {suggested !== null ? (
          <div className="flex flex-wrap items-center gap-3 pb-1">
            <span className="flex flex-col">
              <span className="text-sm text-ink-2">{t("products:priceHelp.suggested")}</span>
              <Amount value={suggested} className="font-bold" />
            </span>
            {onUsePrice ? (
              <Button variant="outline" onClick={() => onUsePrice(suggested)}>
                {t("products:priceHelp.use")}
              </Button>
            ) : null}
          </div>
        ) : null}
      </div>
      {help.error ? (
        <p role="alert" className="text-sm text-danger">
          {errorText(help.error)}
        </p>
      ) : null}
    </section>
  );
}
