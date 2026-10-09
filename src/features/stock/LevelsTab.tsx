import { TriangleAlert } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TruncatedNotice, capped } from "@/components/data-table/list-notices";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { today } from "@/lib/dates";
import { useLevelsView } from "@/lib/ipc/stock-m2";
import type { ExportTarget, LevelRow } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { MinimumsDialog } from "./MinimumsDialog";
import { CategorySelect, LocationSelect, useCan, useLocations } from "./stock-parts";

const col = columnHelper<LevelRow>();
/** Levels listed at most; a search, a location or a category finds the others. */
const LIST_LIMIT = 2_000;

/** STK-002/013/014: the stock of each product at each location against its minimum; the value at
 * the CMUP only with `reports.profit` (the backend sends 0 otherwise), the retail value always. */
export function LevelsTab() {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const { nameOf } = useLocations();
  const [search, setSearch] = useState("");
  const [location, setLocation] = useState("");
  const [category, setCategory] = useState("");
  const [belowMin, setBelowMin] = useState(false);
  const [includeZero, setIncludeZero] = useState(false);
  const [editing, setEditing] = useState<LevelRow | null>(null);
  const text = useDebounced(search.trim(), 200);
  const locationId = location === "" ? null : location;
  const categoryId = category === "" ? null : category;
  // One more than shown: whether the list stops at its limit (sorting a cut list would mislead).
  const levels = useLevelsView({
    text,
    locationId,
    categoryId,
    onlyBelowMin: belowMin,
    includeZero,
    limit: LIST_LIMIT + 1,
  });
  const loaded = capped(levels.data, LIST_LIMIT);
  const costs = can("reports.profit");
  const adjuster = can("stock.adjust");
  const exportTarget: ExportTarget | null = can("reports.stock")
    ? {
        kind: "report",
        request: { kind: "stock_levels", from: today(), to: today(), locationId, categoryId },
      }
    : null;

  const columns = useMemo(
    () =>
      col.columns([
        col.accessor((r) => r.reference ?? "", {
          id: "reference",
          header: () => t("stock:levels.columns.reference"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor((r) => localizedName(r, lang), {
          id: "product",
          header: () => t("stock:levels.columns.product"),
          // stock an archived product still holds is listed, flagged, never out of sight
          cell: (c) =>
            c.row.original.archived ? (
              <span className="flex items-center gap-2">
                <span className="truncate">{c.getValue()}</span>
                <Badge>{t("stock:levels.archived")}</Badge>
              </span>
            ) : (
              c.getValue()
            ),
        }),
        col.accessor((r) => nameOf(r.locationId), {
          id: "location",
          header: () => t("stock:levels.columns.location"),
        }),
        col.accessor("qty", {
          header: () => t("stock:levels.columns.qty"),
          meta: { align: "end" },
          cell: (c) => {
            const below = c.row.original.minQty > 0 && c.getValue() < c.row.original.minQty;
            return (
              <span className={cn("inline-flex items-center gap-1", below && "font-semibold text-warn")}>
                {below ? (
                  <TriangleAlert
                    role="img"
                    aria-label={t("stock:levels.belowTag")}
                    className="size-4 shrink-0"
                  >
                    <title>{t("stock:levels.belowTag")}</title>
                  </TriangleAlert>
                ) : null}
                <bdi dir="ltr" className={cn(c.getValue() < 0 && "text-danger")}>
                  {formatQty(c.getValue(), lang)}
                </bdi>
              </span>
            );
          },
        }),
        col.accessor("minQty", {
          header: () => t("stock:levels.columns.min"),
          meta: { align: "end" },
          cell: (c) => (c.getValue() > 0 ? <bdi dir="ltr">{formatQty(c.getValue(), lang)}</bdi> : null),
        }),
        ...(costs
          ? [
              col.accessor("costValue", {
                header: () => t("stock:levels.columns.costValue"),
                meta: { align: "end" },
                cell: (c) => <Amount value={c.getValue()} />,
              }),
            ]
          : []),
        col.accessor("retailValue", {
          header: () => t("stock:levels.columns.retailValue"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        ...(adjuster
          ? [
              col.display({
                id: "minimums",
                header: () => t("stock:levels.columns.minimums"),
                cell: (c) => (
                  <Button
                    variant="outline"
                    className="min-h-9"
                    aria-label={t("stock:levels.minimumsOf", { name: localizedName(c.row.original, lang) })}
                    onClick={(event) => {
                      event.stopPropagation();
                      setEditing(c.row.original);
                    }}
                  >
                    {t("stock:levels.columns.minimums")}
                  </Button>
                ),
              }),
            ]
          : []),
      ]),
    [t, lang, nameOf, costs, adjuster],
  );

  return (
    <div className="flex flex-col gap-4">
      <TableToolbar
        search={search}
        onSearch={setSearch}
        onScan={setSearch}
        exportTarget={exportTarget}
        filters={
          <div className="flex flex-wrap items-end gap-3">
            <LocationSelect
              id="levels-location"
              label={t("stock:fields.location")}
              value={location}
              onChange={setLocation}
              allLabel={t("stock:fields.allLocations")}
            />
            <CategorySelect
              id="levels-category"
              value={category}
              onChange={setCategory}
              allLabel={t("stock:fields.allCategories")}
            />
            <label className="flex min-h-11 items-center gap-3 font-semibold">
              <Checkbox checked={belowMin} onCheckedChange={(v) => setBelowMin(v === true)} />
              {t("stock:levels.belowMin")}
            </label>
            <label className="flex min-h-11 items-center gap-3 font-semibold">
              <Checkbox checked={includeZero} onCheckedChange={(v) => setIncludeZero(v === true)} />
              {t("stock:levels.includeZero")}
            </label>
          </div>
        }
      />
      {loaded.truncated ? <TruncatedNotice rows={LIST_LIMIT} /> : null}
      <DataTable
        ariaLabel={t("stock:levels.list")}
        data={loaded.rows}
        columns={columns}
        sortable={!loaded.truncated}
        getRowId={(r) => `${r.productId}:${r.locationId}`}
        empty={levels.isPending ? t("common:status.loading") : t("stock:levels.empty")}
      />
      {editing ? (
        <MinimumsDialog
          productId={editing.productId}
          name={localizedName(editing, lang)}
          unitCode={editing.unitCode}
          onClose={() => setEditing(null)}
        />
      ) : null}
    </div>
  );
}
