import { ChevronLeft, ChevronRight } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { DateRangePicker } from "@/components/date-range";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { type DateRange, isDay, rangeOf, today } from "@/lib/dates";
import { formatDate, formatNumber } from "@/lib/format";
import { useMovementLog } from "@/lib/ipc/stock-m2";
import type { ExportTarget, MovementReason, MovementRow } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { LocationSelect, useCan, useLocations, useUserName } from "./stock-parts";

const REASONS: readonly MovementReason[] = [
  "sale",
  "return",
  "purchase",
  "adjustment",
  "count",
  "transfer_in",
  "transfer_out",
  "loss",
  "production",
  "opening",
];
const TONES: Record<MovementReason, "ok" | "info" | "warn" | "danger" | "neutral"> = {
  sale: "info",
  return: "ok",
  purchase: "ok",
  adjustment: "warn",
  count: "warn",
  transfer_in: "neutral",
  transfer_out: "neutral",
  loss: "danger",
  production: "ok",
  opening: "neutral",
};
/** Movements per page, newest first. */
const PAGE = 200;

/** Where a page starts: the instant and the id of the previous page's last row. */
type Cursor = { before: number; beforeId: string } | null;

const col = columnHelper<MovementRow>();

/** STK-015: the append-only stock ledger with who, when, why and the document, filtered by
 * location, reason, period and a search the backend runs over the whole ledger (product, document
 * number, lot, note), paged by keyset (`before` + `beforeId`) 200 at a time. */
export function MovementsTab() {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const userName = useUserName();
  const { nameOf } = useLocations();
  const [location, setLocation] = useState("");
  const [reason, setReason] = useState<MovementReason | "">("");
  const [range, setRange] = useState<DateRange>(() => rangeOf("this_month", today()));
  const [search, setSearch] = useState("");
  // the starts of the pages already seen, the current one last
  const [pages, setPages] = useState<Cursor[]>([null]);
  const cursor = pages[pages.length - 1] ?? null;
  const complete = isDay(range.from) && isDay(range.to);
  const valid = complete && range.from <= range.to;
  const locationId = location === "" ? null : location;
  // The search reads the whole ledger in the backend, not the page shown.
  const text = useDebounced(search.trim(), 250);
  const log = useMovementLog({
    text: text === "" ? null : text,
    locationId,
    reason: reason === "" ? null : reason,
    from: valid ? range.from : null,
    to: valid ? range.to : null,
    before: cursor?.before ?? null,
    beforeId: cursor?.beforeId ?? null,
    limit: PAGE,
  });
  const restart = () => setPages([null]);
  const data = useMemo(() => log.data ?? [], [log.data]);
  const last = data[data.length - 1];
  const exportTarget: ExportTarget | null =
    can("reports.stock") && valid
      ? { kind: "report", request: { kind: "stock_movements", from: range.from, to: range.to, locationId } }
      : null;

  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("createdAt", {
          header: () => t("stock:movements.columns.date"),
          cell: (c) => <bdi dir="ltr">{formatDate(c.getValue(), lang, { withTime: true })}</bdi>,
        }),
        col.accessor((m) => localizedName(m, lang), {
          id: "product",
          header: () => t("stock:movements.columns.product"),
        }),
        col.accessor((m) => nameOf(m.locationId), {
          id: "location",
          header: () => t("stock:movements.columns.location"),
        }),
        col.accessor("qtyDelta", {
          header: () => t("stock:movements.columns.qty"),
          meta: { align: "end" },
          cell: (c) => (
            <bdi dir="ltr" className={cn("font-semibold", c.getValue() < 0 ? "text-danger" : "text-ok")}>
              {`${c.getValue() > 0 ? "+" : ""}${formatQty(c.getValue(), lang)}`}
            </bdi>
          ),
        }),
        col.accessor("reason", {
          header: () => t("stock:movements.columns.reason"),
          cell: (c) => (
            <span className="inline-flex items-center gap-1">
              <Badge tone={TONES[c.getValue()]}>{t(`stock:movements.${c.getValue()}`)}</Badge>
              {c.row.original.reasonCode ? (
                <span className="text-ink-2">{t(`stock:reasons.${c.row.original.reasonCode}`)}</span>
              ) : null}
            </span>
          ),
        }),
        col.accessor((m) => m.refNumber ?? "", {
          id: "document",
          header: () => t("stock:movements.columns.document"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor((m) => m.lotNumber ?? "", {
          id: "lot",
          header: () => t("stock:movements.columns.lot"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor((m) => userName(m.userId), {
          id: "user",
          header: () => t("stock:movements.columns.user"),
        }),
        col.accessor((m) => m.note ?? "", {
          id: "note",
          header: () => t("stock:movements.columns.note"),
          enableSorting: false,
        }),
      ]),
    [t, lang, nameOf, userName],
  );

  return (
    <div className="flex flex-col gap-4">
      <TableToolbar
        search={search}
        onSearch={(next) => {
          setSearch(next);
          restart();
        }}
        onScan={(code) => {
          setSearch(code);
          restart();
        }}
        exportTarget={exportTarget}
        filters={
          <div className="flex flex-wrap items-end gap-3">
            <LocationSelect
              id="movements-location"
              label={t("stock:fields.location")}
              value={location}
              onChange={(id) => {
                setLocation(id);
                restart();
              }}
              allLabel={t("stock:fields.allLocations")}
            />
            <Field id="movements-reason" label={t("stock:movements.columns.reason")}>
              <NativeSelect
                id="movements-reason"
                value={reason}
                onChange={(event) => {
                  setReason(REASONS.find((r) => r === event.target.value) ?? "");
                  restart();
                }}
              >
                <option value="">{t("stock:movements.allReasons")}</option>
                {REASONS.map((r) => (
                  <option key={r} value={r}>
                    {t(`stock:movements.${r}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          </div>
        }
      />
      <DateRangePicker
        value={range}
        onChange={(next) => {
          setRange(next);
          restart();
        }}
        error={complete && !valid ? t("validation:rangeOrder") : undefined}
      />
      <DataTable
        ariaLabel={t("stock:movements.list")}
        data={data}
        columns={columns}
        getRowId={(m) => m.id}
        empty={log.isPending ? t("common:status.loading") : t("stock:movements.empty")}
      />
      <nav aria-label={t("stock:movements.list")} className="flex flex-wrap items-center justify-end gap-2">
        <Button
          variant="outline"
          disabled={pages.length === 1}
          onClick={() => setPages((all) => all.slice(0, -1))}
        >
          <ChevronLeft aria-hidden className="rtl:-scale-x-100" />
          {t("stock:movements.newer")}
        </Button>
        <span className="text-ink-2">
          {t("stock:movements.page", { page: formatNumber(pages.length, lang) })}
        </span>
        <Button
          variant="outline"
          disabled={data.length < PAGE || last === undefined}
          onClick={() => {
            if (last) setPages((all) => [...all, { before: last.createdAt, beforeId: last.id }]);
          }}
        >
          {t("stock:movements.older")}
          <ChevronRight aria-hidden className="rtl:-scale-x-100" />
        </Button>
      </nav>
    </div>
  );
}
