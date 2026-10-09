import {
  type ColumnDef,
  type RowData,
  createColumnHelper,
  createSortedRowModel,
  rowSortingFeature,
  sortFn_alphanumeric,
  sortFn_text,
  tableFeatures,
  useTable,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp, ArrowUpDown } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";

/** Per-column options: `align: "end"` for amounts and quantities (spec §10.4). */
export interface ColumnMeta {
  align?: "start" | "end";
}

/** Sorting only; the sort functions `sortFn: "auto"` picks for text are registered (numbers use
 * the built-in basic comparison). */
export const features = tableFeatures({
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
  sortFns: { alphanumeric: sortFn_alphanumeric, text: sortFn_text },
  columnMeta: {} as ColumnMeta,
});
// eslint-disable-next-line @typescript-eslint/no-explicit-any -- column value types differ per column
export type Columns<T extends RowData> = Array<ColumnDef<typeof features, T, any>>;
export const columnHelper = <T extends RowData>() => createColumnHelper<typeof features, T>();

/** Every column at least this wide; a wider table scrolls sideways inside its frame. */
const MIN_COLUMN_REM = 7;

/** The grid of a table's rows, for a footer row laid out like the others. */
export function gridColumns(count: number): string {
  return `repeat(${count}, minmax(${MIN_COLUMN_REM}rem, 1fr))`;
}

interface Props<T extends RowData> {
  ariaLabel: string;
  data: T[];
  columns: Columns<T>;
  getRowId: (row: T) => string;
  rowHeight?: number;
  onRowClick?: (row: T) => void;
  /** Shown instead of the rows when there are none (default "No rows"). */
  empty?: ReactNode;
  /** Pinned under the rows: a `role="row"` laid out with `gridColumns`. */
  footer?: ReactNode;
  /** `false` when the rows are a cut list (the first N of more): sorting them would rank only
   * what was loaded, so the rows keep the backend's order. */
  sortable?: boolean;
}

/**
 * TanStack Table v9 with the sorting feature, rows virtualised by TanStack Virtual (only the
 * visible rows are in the DOM), a sticky header, `aria-sort` on sortable headers and keyboard row
 * focus: ↑/↓, Home/End move between rows, Enter opens the row.
 */
export function DataTable<T extends RowData>({
  ariaLabel,
  data,
  columns,
  getRowId,
  rowHeight = 44,
  onRowClick,
  empty,
  footer,
  sortable = true,
}: Props<T>) {
  const { t } = useTranslation();
  // Every column sorts ascending on the first click, numbers too (TanStack starts them descending).
  const table = useTable({
    features,
    columns,
    data,
    getRowId: (row) => getRowId(row),
    sortDescFirst: false,
    enableSorting: sortable,
    manualSorting: !sortable,
  });
  const scroller = useRef<HTMLDivElement>(null);
  const rows = table.getRowModel().rows;
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual (spec §10.4)
  const virtual = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => rowHeight,
    overscan: 10,
  });
  const items = virtual.getVirtualItems();
  const [active, setActive] = useState(0);
  const pendingFocus = useRef<number | null>(null);
  const current = Math.min(active, Math.max(rows.length - 1, 0));
  // One row takes Tab (roving tabindex): the active one, or the first shown when it scrolled away.
  const tabStop = items.some((item) => item.index === current) ? current : items[0]?.index;
  const grid = gridColumns(columns.length);

  useEffect(() => {
    if (pendingFocus.current === null) return;
    const row = scroller.current?.querySelector<HTMLElement>(`[data-index="${pendingFocus.current}"]`);
    if (row) {
      pendingFocus.current = null;
      row.focus({ preventScroll: true });
    }
  });

  const moveTo = (index: number) => {
    const next = Math.max(0, Math.min(rows.length - 1, index));
    setActive(next);
    pendingFocus.current = next;
    virtual.scrollToIndex(next);
  };
  const onRowKey = (event: KeyboardEvent<HTMLDivElement>, index: number, row: T) => {
    const target: Record<string, number> = {
      ArrowDown: index + 1,
      ArrowUp: index - 1,
      Home: 0,
      End: rows.length - 1,
    };
    const next = target[event.key];
    if (next !== undefined) {
      event.preventDefault();
      moveTo(next);
    } else if (event.key === "Enter" && onRowClick) {
      event.preventDefault();
      onRowClick(row);
    }
  };

  return (
    <div
      ref={scroller}
      className="relative h-full max-h-[70dvh] min-h-0 overflow-auto rounded-card border border-line bg-surface"
    >
      <div
        role="table"
        aria-label={ariaLabel}
        aria-rowcount={rows.length + (footer ? 2 : 1)}
        className="w-full"
        style={{ minWidth: `${columns.length * MIN_COLUMN_REM}rem` }}
      >
        <div role="rowgroup" className="sticky top-0 z-10 bg-surface-2">
          {table.getHeaderGroups().map((group) => (
            <div
              role="row"
              key={group.id}
              aria-rowindex={1}
              className="grid border-b border-line"
              style={{ gridTemplateColumns: grid }}
            >
              {group.headers.map((header) => {
                const column = header.column;
                const sorted = column.getIsSorted();
                const end = column.columnDef.meta?.align === "end";
                return (
                  <div
                    role="columnheader"
                    key={header.id}
                    aria-sort={
                      column.getCanSort()
                        ? sorted === "asc"
                          ? "ascending"
                          : sorted === "desc"
                            ? "descending"
                            : "none"
                        : undefined
                    }
                    className={cn(
                      "flex min-h-11 items-center px-3 text-sm font-semibold text-ink-2",
                      end ? "justify-end text-end" : "text-start",
                    )}
                  >
                    {header.isPlaceholder ? null : column.getCanSort() ? (
                      <button
                        type="button"
                        onClick={column.getToggleSortingHandler()}
                        className="inline-flex min-h-11 items-center gap-1 rounded-sm focus-visible:outline-3 focus-visible:outline-focus"
                      >
                        <table.FlexRender header={header} />
                        {sorted === "asc" ? (
                          <ArrowUp aria-hidden className="size-4 shrink-0" />
                        ) : sorted === "desc" ? (
                          <ArrowDown aria-hidden className="size-4 shrink-0" />
                        ) : (
                          <ArrowUpDown aria-hidden className="size-4 shrink-0 opacity-40" />
                        )}
                      </button>
                    ) : (
                      <table.FlexRender header={header} />
                    )}
                  </div>
                );
              })}
            </div>
          ))}
        </div>
        {rows.length === 0 ? (
          <div role="rowgroup">
            <div role="row" aria-rowindex={2}>
              <div role="cell" aria-colspan={columns.length} className="p-6 text-center text-ink-2">
                {empty ?? t("tables:empty")}
              </div>
            </div>
          </div>
        ) : (
          <div role="rowgroup" className="relative" style={{ height: virtual.getTotalSize() }}>
            {items.map((item) => {
              const row = rows[item.index];
              if (!row) return null;
              return (
                <div
                  role="row"
                  key={row.id}
                  data-index={item.index}
                  aria-rowindex={item.index + 2}
                  tabIndex={item.index === tabStop ? 0 : -1}
                  onFocus={() => setActive(item.index)}
                  onClick={onRowClick ? () => onRowClick(row.original) : undefined}
                  onKeyDown={(event) => onRowKey(event, item.index, row.original)}
                  className={cn(
                    "absolute inset-x-0 top-0 grid items-center border-b border-line focus-visible:outline-3 focus-visible:-outline-offset-3 focus-visible:outline-focus",
                    item.index % 2 === 1 && "bg-surface-2",
                    onRowClick && "cursor-pointer hover:bg-brand-soft",
                  )}
                  style={{
                    gridTemplateColumns: grid,
                    height: rowHeight,
                    transform: `translateY(${item.start}px)`,
                  }}
                >
                  {row.getAllCells().map((cell) => (
                    <div
                      role="cell"
                      key={cell.id}
                      className={cn(
                        "truncate px-3 text-sm",
                        cell.column.columnDef.meta?.align === "end" && "text-end tabular-nums",
                      )}
                    >
                      <table.FlexRender cell={cell} />
                    </div>
                  ))}
                </div>
              );
            })}
          </div>
        )}
        {footer ? (
          <div role="rowgroup" className="sticky bottom-0 border-t border-line bg-surface-2 font-semibold">
            {footer}
          </div>
        ) : null}
      </div>
    </div>
  );
}
