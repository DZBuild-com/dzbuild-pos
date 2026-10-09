import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { formatDZD } from "@/lib/format";
import { useChifaOf, useEquivalents } from "@/lib/ipc/pharmacy";
import { useLang } from "@/lib/use-language";
import type { TillProduct } from "./PosDialogs";

/**
 * Pharmacy, under a medicine's line: a cheaper generic of the same DCI in stock ("Remplacer" puts
 * it in place of the line, as a journaled void and a new line), and the Chifa reference price
 * when known (informative only: the till never computes a reimbursement).
 */
export function LineNotices({
  productId,
  onReplace,
}: {
  productId: string;
  onReplace: (generic: TillProduct) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const equivalents = useEquivalents(productId);
  const chifa = useChifaOf(productId);
  const generic = equivalents.data?.find((e) => e.cheaper && e.onHand > 0);
  if (!generic && !chifa.data) return null;
  return (
    <div className="flex flex-col gap-1 border-b border-line px-4 pb-2 text-sm">
      {generic ? (
        <p className="flex items-center justify-between gap-2 text-info">
          <span>
            {t("packs:till.generic", { name: generic.name, price: formatDZD(generic.price, lang) })}
          </span>
          <Button
            variant="outline"
            onClick={() =>
              onReplace({
                id: generic.productId,
                kind: "simple",
                unitCode: "pc",
                sellPrice: generic.price,
                nameAr: generic.name,
                nameFr: generic.name,
                nameEn: generic.name,
              })
            }
          >
            {t("packs:till.replace")}
          </Button>
        </p>
      ) : null}
      {chifa.data ? (
        <p className="text-ink-2">
          {t("packs:till.chifa", { price: formatDZD(chifa.data.referencePrice, lang) })}
        </p>
      ) : null}
    </div>
  );
}
