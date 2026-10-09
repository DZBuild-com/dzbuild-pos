import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { ExportMenu } from "@/components/data-table/export-menu";
import { RadioCards } from "@/components/radio-cards";
import { useCan, useLocations } from "@/features/stock/stock-parts";
import { dayToMs, today } from "@/lib/dates";
import { formatDZD, formatDate } from "@/lib/format";
import { useExpiryBoard } from "@/lib/ipc/pharmacy";
import type { BoardLot, ExportTarget } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** The horizons the pharmacist chooses from (30 days by default). */
const HORIZONS = ["7", "30", "60", "90"] as const;
type Horizon = (typeof HORIZONS)[number];

const col = columnHelper<BoardLot>();

/**
 * PRD-053 / VRT-030: the pharmacy's expiry board above M2's lot list. Two tables, the lots already
 * expired first, then the ones expiring within the horizon (7, 30, 60 or 90 days), each lot with
 * its value at cost and each table with the cost at risk (both from the backend, shown to whoever
 * sees costs). Export and print are M2's expiry report.
 */
export function ExpiryBoard() {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const errorText = useErrorText();
  const { nameOf } = useLocations();
  const [horizon, setHorizon] = useState<Horizon>("30");
  const days = Number(horizon);
  const board = useExpiryBoard(days);
  const seesCosts = can("reports.profit");
  const exportTarget: ExportTarget | null = can("reports.stock")
    ? { kind: "report", request: { kind: "expiry", from: today(), to: today() } }
    : null;

  const columns = useMemo(
    () =>
      col.columns([
        col.accessor((r) => localizedName(r.lot, lang), {
          id: "product",
          header: () => t("pharmacy:expiry.product"),
        }),
        col.accessor((r) => r.lot.lotNumber, {
          id: "lot",
          header: () => t("pharmacy:expiry.lot"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor((r) => r.lot.expiryDate, {
          id: "date",
          header: () => t("pharmacy:expiry.date"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        col.accessor((r) => r.lot.qty, {
          id: "qty",
          header: () => t("pharmacy:expiry.qty"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{formatQty(c.getValue(), lang)}</bdi>,
        }),
        ...(seesCosts
          ? [
              col.accessor("cost", {
                header: () => t("pharmacy:expiry.value"),
                meta: { align: "end" },
                cell: (c) => <Amount value={c.getValue()} />,
              }),
            ]
          : []),
        col.accessor((r) => nameOf(r.lot.locationId), {
          id: "location",
          header: () => t("pharmacy:expiry.location"),
        }),
      ]),
    [t, lang, nameOf, seesCosts],
  );

  if (board.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(board.error)}
      </p>
    );
  }
  if (!board.data) return null;
  const rowId = (r: BoardLot) => `${r.lot.lotId}:${r.lot.locationId}`;
  const total = (amount: number) =>
    seesCosts ? (
      <p className="font-semibold">{t("pharmacy:expiry.cost", { amount: formatDZD(amount, lang) })}</p>
    ) : null;
  return (
    <section className="flex flex-col gap-4" aria-labelledby="expiry-board-title">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <h2 id="expiry-board-title" className="text-2xl font-bold">
          {t("pharmacy:expiry.title")}
        </h2>
        {exportTarget ? <ExportMenu target={exportTarget} /> : null}
      </div>
      <RadioCards<Horizon>
        legend={t("pharmacy:expiry.horizon")}
        name="expiry-horizon"
        value={horizon}
        onChange={setHorizon}
        options={HORIZONS.map((d) => ({
          value: d,
          label: t("pharmacy:expiry.within", { count: Number(d) }),
        }))}
      />
      <h3 className="text-xl font-semibold">{t("pharmacy:expiry.expired")}</h3>
      <DataTable
        ariaLabel={t("pharmacy:expiry.expired")}
        data={board.data.expired}
        columns={columns}
        getRowId={rowId}
        empty={t("pharmacy:expiry.emptyExpired")}
      />
      {total(board.data.expiredCost)}
      <h3 className="text-xl font-semibold">{t("pharmacy:expiry.soon", { count: days })}</h3>
      <DataTable
        ariaLabel={t("pharmacy:expiry.soon", { count: days })}
        data={board.data.soon}
        columns={columns}
        getRowId={rowId}
        empty={t("pharmacy:expiry.emptySoon")}
      />
      {total(board.data.soonCost)}
    </section>
  );
}
