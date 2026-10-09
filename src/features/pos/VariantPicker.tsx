import { useTranslation } from "react-i18next";
import { Dialog } from "@/components/ui/dialog";
import { useVariantGrid } from "@/lib/ipc/till-packs";
import type { GridCell } from "@/lib/ipc/types";
import type { ProductHit } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import type { TillProduct } from "./PosDialogs";
import { useTillKit } from "./till-kit";

/**
 * SAL-023: the size × colour grid of a variant model (`variant_grid`), each cell with its stock
 * (the van's when the till sells from one); an empty cell is disabled; a cell adds that variant.
 */
export function VariantPicker({
  parent,
  onPick,
  onClose,
}: {
  parent: ProductHit;
  onPick: (variant: TillProduct) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const van = useTillKit().useExtras((s) => s.van);
  const grid = useVariantGrid(parent.id, van?.locationId ?? null);
  const name = localizedName(parent, lang);
  const data = grid.data;
  // a one-axis model is a single column
  const cols = data && data.colAxis !== null ? data.cols : [null];
  const cellOf = (row: string, col: string | null): GridCell | undefined =>
    data?.cells.find((c) => c.rowValue === row && c.colValue === col);
  const pick = (cell: GridCell) => {
    const suffix = [cell.rowValue, cell.colValue].filter(Boolean).join(" ");
    onPick({
      id: cell.variantId,
      kind: "variant",
      unitCode: parent.unitCode,
      sellPrice: cell.price,
      nameAr: `${parent.nameAr} ${suffix}`,
      nameFr: `${parent.nameFr} ${suffix}`,
      nameEn: `${parent.nameEn} ${suffix}`,
    });
  };
  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("packs:till.variants.pick")}
      description={name}
    >
      {grid.error ? (
        <p role="alert" className="text-danger">
          {errorText(grid.error)}
        </p>
      ) : null}
      {data ? (
        <div role="grid" aria-label={name} className="flex flex-col gap-2 overflow-auto">
          <div role="row" className="flex gap-2">
            <span role="columnheader" className="w-20 shrink-0 font-semibold text-ink-2">
              {data.rowAxis}
            </span>
            {cols.map((col) => (
              <span key={col ?? ""} role="columnheader" className="min-w-20 flex-1 text-center font-semibold">
                {col ?? data.rowAxis}
              </span>
            ))}
          </div>
          {data.rows.map((row) => (
            <div key={row} role="row" className="flex gap-2">
              <span role="rowheader" className="grid w-20 shrink-0 place-items-center font-semibold">
                {row}
              </span>
              {cols.map((col) => {
                const cell = cellOf(row, col);
                const stock = formatQty(cell?.onHand ?? 0, lang);
                return (
                  <button
                    key={col ?? ""}
                    type="button"
                    role="gridcell"
                    aria-label={t("packs:till.variants.cell", { row, col: col ?? "", stock })}
                    disabled={!cell || cell.onHand <= 0}
                    onClick={() => (cell ? pick(cell) : undefined)}
                    className={cn(
                      "min-h-14 min-w-20 flex-1 rounded-control border border-line font-bold tabular-nums",
                      "bg-surface hover:bg-brand-soft disabled:bg-surface-2 disabled:text-ink-2",
                    )}
                  >
                    {cell ? <bdi dir="ltr">{stock}</bdi> : "—"}
                  </button>
                );
              })}
            </div>
          ))}
        </div>
      ) : null}
    </Dialog>
  );
}
