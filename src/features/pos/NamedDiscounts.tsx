import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { usePresets } from "@/lib/ipc/promotions";
import { localizedName } from "@/lib/names";
import { formatRate } from "@/lib/numeric";
import { useLang } from "@/lib/use-language";

/** PRM-003: the named discounts of the day ("Remise fidèle 5 %") as one-tap buttons. A named
 * discount is a manual percent (manual promotions are percents only): the backend still checks the
 * cashier's discount limit and asks a manager above it. */
export function NamedDiscounts({ onPick }: { onPick: (rateBp: number) => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const presets = usePresets();
  const named = (presets.data ?? []).filter((p) => p.kind === "percent" && p.value > 0);
  if (named.length === 0) return null;
  return (
    <div role="group" aria-label={t("pos:namedDiscounts.title")} className="flex flex-col gap-2">
      <span className="font-semibold">{t("pos:namedDiscounts.title")}</span>
      <div className="flex flex-wrap gap-2">
        {named.map((preset) => (
          <Button
            key={preset.id}
            variant="secondary"
            className="min-h-14"
            onClick={() => onPick(preset.value)}
          >
            {localizedName(preset, lang)}
            <bdi dir="ltr" className="font-normal">
              {formatRate(preset.value, lang)}
            </bdi>
          </Button>
        ))}
      </div>
    </div>
  );
}
