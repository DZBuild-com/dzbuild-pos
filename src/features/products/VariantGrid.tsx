import { ArrowLeftRight, Grid3x3 } from "lucide-react";
import { useEffect, useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { LocationSelect } from "@/features/stock/stock-parts";
import { useUnits } from "@/lib/ipc/catalog";
import { useProductSearch, useVariants } from "@/lib/ipc/products";
import type { ProductHit } from "@/lib/ipc/types";
import { useVariantGridOf } from "@/lib/ipc/completion";
import type { GridCell } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty, parseQty } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** A quantity typed in a cell of the grid, for one variant. */
export interface GridLine {
  variantId: string;
  qty: number;
}

/** A cell's place in the grid: its row and column values. */
function cellKey(row: string, col: string | null): string {
  return JSON.stringify([row, col]);
}

/** The axis chosen for the rows of a model, kept on this PC (`null`: the model's first axis). */
function storageKey(parentId: string): string {
  return `variantGrid.${parentId}`;
}

function readRowAxis(parentId: string): string | null {
  try {
    return window.localStorage.getItem(storageKey(parentId));
  } catch {
    return null;
  }
}

function forgetRowAxis(parentId: string): void {
  try {
    window.localStorage.removeItem(storageKey(parentId));
  } catch {
    // nothing kept, nothing to forget
  }
}

function keepRowAxis(parentId: string, axis: string): void {
  try {
    window.localStorage.setItem(storageKey(parentId), axis);
  } catch {
    // a private window or blocked storage: the choice lasts for this screen only
  }
}

/**
 * VRT-060: a variant model's stock as a grid, one row per value of its row axis and one column per
 * value of the other (size × colour), at a location or everywhere; "Inverser les axes" turns it and
 * the choice is kept per model on this PC. With `onEnter` (a count, a goods receipt) every cell
 * takes a quantity and "Ajouter les lignes" hands back one line per filled cell.
 */
export function VariantGrid({
  parentId,
  onEnter,
  decimals = 0,
}: {
  parentId: string;
  onEnter?: (lines: GridLine[]) => void;
  /** The decimals a quantity of the model's unit may have (0 for pieces). */
  decimals?: number;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const id = useId();
  const [rowAxis, setRowAxis] = useState<string | null>(() => readRowAxis(parentId));
  const [location, setLocation] = useState("");
  // Typed quantities by variant: turning the grid keeps them.
  const [typed, setTyped] = useState<Record<string, string>>({});
  const [invalid, setInvalid] = useState<ReadonlySet<string>>(new Set());
  const grid = useVariantGridOf(parentId, location === "" ? null : location, rowAxis);
  const data = grid.data;
  const cells = useMemo(() => {
    const byKey = new Map<string, GridCell>();
    for (const cell of data?.cells ?? []) byKey.set(cellKey(cell.rowValue, cell.colValue), cell);
    return byKey;
  }, [data]);
  const columns: (string | null)[] = data && data.cols.length > 0 ? data.cols : [null];

  // An axis kept on this PC that the model no longer has: the backend answered with its first
  // axis, and the choice kept here goes.
  const staleAxis =
    rowAxis !== null && data !== undefined && !grid.isPlaceholderData && data.rowAxis !== rowAxis;
  useEffect(() => {
    if (staleAxis) forgetRowAxis(parentId);
  }, [staleAxis, parentId]);

  const swap = () => {
    if (!data?.colAxis) return;
    setRowAxis(data.colAxis);
    keepRowAxis(parentId, data.colAxis);
  };
  const add = () => {
    if (!data || !onEnter) return;
    const lines: GridLine[] = [];
    const wrong = new Set<string>();
    for (const row of data.rows) {
      for (const col of columns) {
        const cell = cells.get(cellKey(row, col));
        const text = cell ? (typed[cell.variantId] ?? "").trim() : "";
        if (!cell || text === "") continue;
        const qty = parseQty(text, decimals);
        if (qty === null) wrong.add(cell.variantId);
        else lines.push({ variantId: cell.variantId, qty });
      }
    }
    setInvalid(wrong);
    if (wrong.size === 0 && lines.length > 0) onEnter(lines);
  };

  if (grid.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(grid.error)}
      </p>
    );
  }
  if (!data) return null;
  if (data.cells.length === 0) return <p className="text-ink-2">{t("products:variantGrid.empty")}</p>;
  const label = (row: string, col: string | null) => (col === null ? row : `${row} ${col}`);
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end gap-3">
        {onEnter ? null : (
          <div className="min-w-56">
            <LocationSelect
              id={`${id}-location`}
              label={t("products:variantGrid.location")}
              value={location}
              onChange={setLocation}
              allLabel={t("products:variantGrid.allLocations")}
            />
          </div>
        )}
        {data.colAxis ? (
          <Button variant="outline" onClick={swap}>
            <ArrowLeftRight aria-hidden />
            {t("products:variantGrid.swap")}
          </Button>
        ) : null}
      </div>
      <div className="overflow-x-auto rounded-card border border-line bg-surface">
        <div
          role="grid"
          aria-label={t("products:variantGrid.title")}
          className="grid min-w-max"
          style={{ gridTemplateColumns: `minmax(6rem, auto) repeat(${columns.length}, minmax(6rem, 1fr))` }}
        >
          <div role="row" className="contents">
            <div role="columnheader" className="bg-surface-2 px-3 py-2 text-sm font-semibold text-ink-2">
              {data.colAxis ? `${data.rowAxis} / ${data.colAxis}` : data.rowAxis}
            </div>
            {columns.map((col) => (
              <div
                role="columnheader"
                key={col ?? "-"}
                className="bg-surface-2 px-3 py-2 text-end text-sm font-semibold text-ink-2"
              >
                {col ?? t("products:columns.stock")}
              </div>
            ))}
          </div>
          {data.rows.map((row, index) => (
            <div role="row" key={row} className="contents">
              <div
                role="rowheader"
                className={cn(
                  "border-t border-line px-3 py-2 font-semibold",
                  index % 2 === 1 && "bg-surface-2",
                )}
              >
                {row}
              </div>
              {columns.map((col) => {
                const cell = cells.get(cellKey(row, col));
                const zebra = index % 2 === 1 && "bg-surface-2";
                if (!cell) {
                  return (
                    <div
                      role="gridcell"
                      key={col ?? "-"}
                      aria-label={label(row, col)}
                      className={cn("border-t border-line", zebra)}
                    />
                  );
                }
                if (onEnter) {
                  return (
                    <div role="gridcell" key={col ?? "-"} className={cn("border-t border-line p-1", zebra)}>
                      <NumericInput
                        kind={decimals === 0 ? "integer" : "qty"}
                        aria-label={label(row, col)}
                        aria-invalid={invalid.has(cell.variantId)}
                        value={typed[cell.variantId] ?? ""}
                        onValueChange={(value) => setTyped((all) => ({ ...all, [cell.variantId]: value }))}
                        className={cn("h-10 text-end", invalid.has(cell.variantId) && "border-danger")}
                      />
                    </div>
                  );
                }
                const stock = formatQty(cell.onHand, lang);
                return (
                  <div
                    role="gridcell"
                    key={col ?? "-"}
                    aria-label={`${label(row, col)} ${stock}`}
                    className={cn(
                      "border-t border-line px-3 py-2 text-end tabular-nums",
                      zebra,
                      cell.onHand <= 0 && "text-danger",
                    )}
                  >
                    <bdi dir="ltr">{stock}</bdi>
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      </div>
      {data.otherAxes.length > 0 ? (
        <p className="text-sm text-ink-2">
          {t("products:variantGrid.otherAxes", { axes: data.otherAxes.join(", ") })}
        </p>
      ) : null}
      {onEnter ? (
        <div className="flex justify-end">
          <Button variant="primary" onClick={add}>
            {t("products:variantGrid.addLines")}
          </Button>
        </div>
      ) : null}
    </div>
  );
}

/** A line of a count or a goods receipt typed in the grid, with the variant's name and unit. */
export interface EnteredLine extends GridLine {
  name: string;
  unitCode: string;
}

/**
 * "Saisir par grille" (VRT-060) in M2's count sheet and goods receipt: find the variant model, type
 * a quantity per size and colour, and one line per filled cell comes back to the editor.
 */
export function VariantGridDialog({
  onAdd,
  onClose,
}: {
  onAdd: (lines: EnteredLine[]) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const id = useId();
  const [text, setText] = useState("");
  const [model, setModel] = useState<ProductHit | null>(null);
  const query = useDebounced(text.trim(), 200);
  const search = useProductSearch(
    { text: query, limit: 20, categoryId: null, includeArchived: false },
    model === null && query.length >= 2,
  );
  const models = (search.data ?? []).filter((hit) => hit.kind === "variant_parent");
  const variants = useVariants(model?.id ?? null);
  const decimals = useUnits().data?.find((u) => u.code === model?.unitCode)?.decimals ?? 0;
  const add = (lines: GridLine[]) => {
    const byId = new Map((variants.data ?? []).map((v) => [v.id, v]));
    onAdd(
      lines.map((line) => {
        const variant = byId.get(line.variantId);
        return {
          ...line,
          name: variant ? localizedName(variant, lang) : model ? localizedName(model, lang) : "",
          unitCode: variant?.unitCode ?? model?.unitCode ?? "pc",
        };
      }),
    );
  };
  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={model ? localizedName(model, lang) : t("products:variantGrid.enter")}
    >
      {model ? (
        <VariantGrid parentId={model.id} onEnter={add} decimals={decimals} />
      ) : (
        <div className="flex flex-col gap-3">
          <Field id={`${id}-model`} label={t("products:variantGrid.model")}>
            <Input
              id={`${id}-model`}
              type="search"
              dir="auto"
              autoComplete="off"
              autoFocus
              value={text}
              onChange={(event) => setText(event.target.value)}
            />
          </Field>
          {query.length >= 2 && search.isSuccess && models.length === 0 ? (
            <p className="text-ink-2">{t("products:variantGrid.noModel")}</p>
          ) : null}
          <ul className="flex flex-wrap gap-2">
            {models.map((hit) => (
              <li key={hit.id}>
                <Button variant="outline" onClick={() => setModel(hit)}>
                  <Grid3x3 aria-hidden />
                  {localizedName(hit, lang)}
                </Button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </Dialog>
  );
}
