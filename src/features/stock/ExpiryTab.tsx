import { Suspense, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { NumericInput } from "@/components/numeric-input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { ExpiryBoard } from "@/features/packs/pack-screens";
import { dayToMs, today } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useModule } from "@/lib/ipc/pack-state";
import { useAdjustBatch, useExpiry } from "@/lib/ipc/stock-m2";
import type { ExpiryRow, ExpiryStatus, ExportTarget } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { LocationSelect, useCan, useLocations } from "./stock-parts";

const STATUSES: readonly ExpiryStatus[] = ["expired", "soon"];

const col = columnHelper<ExpiryRow>();

/** PRD-052: lots expired or expiring within the alert delay (`stock.expiry_alert_days`, or the
 * horizon typed here), with a quick "Retirer (périmé)" adjustment of an expired lot. A pharmacy
 * (its module on) sees its expiry board above (PRD-053, VRT-030). */
export function ExpiryTab() {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const { nameOf } = useLocations();
  const [within, setWithin] = useState("");
  const days = useDebounced(within.trim() === "" ? null : Number(within), 300);
  const expiry = useExpiry(days);
  const [status, setStatus] = useState<ExpiryStatus | "">("");
  const [location, setLocation] = useState("");
  const [search, setSearch] = useState("");
  const [removing, setRemoving] = useState<ExpiryRow | null>(null);
  const adjuster = can("stock.adjust");
  const pharmacy = useModule("pharmacy");

  const text = search.trim().toLowerCase();
  const rows = useMemo(
    () =>
      (expiry.data ?? []).filter(
        (r) =>
          (status === "" || r.status === status) &&
          (location === "" || r.locationId === location) &&
          (text === "" ||
            [r.nameFr, r.nameAr, r.nameEn, r.lotNumber].some((v) => v.toLowerCase().includes(text))),
      ),
    [expiry.data, status, location, text],
  );
  const exportTarget: ExportTarget | null = can("reports.stock")
    ? {
        kind: "report",
        request: {
          kind: "expiry",
          from: today(),
          to: today(),
          locationId: location === "" ? null : location,
        },
      }
    : null;

  const columns = useMemo(
    () =>
      col.columns([
        col.accessor((r) => localizedName(r, lang), {
          id: "product",
          header: () => t("stock:expiry.columns.product"),
        }),
        col.accessor("lotNumber", {
          header: () => t("stock:expiry.columns.lot"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("expiryDate", {
          header: () => t("stock:expiry.columns.expiryDate"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        col.accessor("daysLeft", {
          header: () => t("stock:expiry.columns.days"),
          cell: (c) => {
            const left = c.getValue();
            return left >= 0
              ? t("stock:expiry.in", { count: left })
              : t("stock:expiry.ago", { count: -left });
          },
        }),
        col.accessor((r) => nameOf(r.locationId), {
          id: "location",
          header: () => t("stock:expiry.columns.location"),
        }),
        col.accessor("qty", {
          header: () => t("stock:expiry.columns.qty"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{formatQty(c.getValue(), lang)}</bdi>,
        }),
        col.accessor("status", {
          header: () => t("stock:expiry.columns.status"),
          cell: (c) => (
            <Badge tone={c.getValue() === "expired" ? "danger" : "warn"}>
              {t(`stock:expiry.status.${c.getValue()}`)}
            </Badge>
          ),
        }),
        ...(adjuster
          ? [
              col.display({
                id: "action",
                header: () => t("stock:expiry.columns.action"),
                cell: (c) =>
                  c.row.original.status === "expired" ? (
                    <Button
                      variant="danger"
                      className="min-h-9"
                      aria-label={t("stock:expiry.removeOf", {
                        name: localizedName(c.row.original, lang),
                        lot: c.row.original.lotNumber,
                      })}
                      onClick={(event) => {
                        event.stopPropagation();
                        setRemoving(c.row.original);
                      }}
                    >
                      {t("stock:expiry.remove")}
                    </Button>
                  ) : null,
              }),
            ]
          : []),
      ]),
    [t, lang, nameOf, adjuster],
  );

  return (
    <div className="flex flex-col gap-4">
      {pharmacy ? (
        <Suspense fallback={null}>
          <ExpiryBoard />
        </Suspense>
      ) : null}
      <TableToolbar
        search={search}
        onSearch={setSearch}
        exportTarget={exportTarget}
        filters={
          <div className="flex flex-wrap items-end gap-3">
            <LocationSelect
              id="expiry-location"
              label={t("stock:fields.location")}
              value={location}
              onChange={setLocation}
              allLabel={t("stock:fields.allLocations")}
            />
            <Field id="expiry-status" label={t("stock:fields.status")}>
              <NativeSelect
                id="expiry-status"
                value={status}
                onChange={(event) => setStatus(STATUSES.find((s) => s === event.target.value) ?? "")}
              >
                <option value="">{t("stock:fields.allStatuses")}</option>
                {STATUSES.map((s) => (
                  <option key={s} value={s}>
                    {t(`stock:expiry.status.${s}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            <Field
              id="expiry-within"
              label={t("stock:expiry.withinDays")}
              hint={t("stock:expiry.withinHint")}
            >
              <NumericInput
                id="expiry-within"
                kind="integer"
                maxLength={3}
                className="w-28"
                value={within}
                onValueChange={setWithin}
              />
            </Field>
          </div>
        }
      />
      <DataTable
        ariaLabel={t("stock:expiry.list")}
        data={rows}
        columns={columns}
        getRowId={(r) => `${r.lotId}:${r.locationId}`}
        empty={expiry.isPending ? t("common:status.loading") : t("stock:expiry.empty")}
      />
      {removing ? <RemoveExpiredDialog row={removing} onClose={() => setRemoving(null)} /> : null}
    </div>
  );
}

/** "Retirer (périmé)": the lot's quantity leaves its location with the reason `expiry` (a batch
 * adjustment; the backend takes the product's expired lots first). */
function RemoveExpiredDialog({ row, onClose }: { row: ExpiryRow; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const { nameOf } = useLocations();
  const adjust = useAdjustBatch();
  const name = localizedName(row, lang);
  const submit = () =>
    adjust.mutate(
      {
        locationId: row.locationId,
        reason: "expiry",
        lines: [{ productId: row.productId, qtyDelta: -row.qty, unitCost: null }],
        note: t("stock:expiry.note", { lot: row.lotNumber }),
      },
      {
        onSuccess: () => {
          toast.success(t("stock:expiry.removed", { lot: row.lotNumber }));
          onClose();
        },
      },
    );
  return (
    <Dialog
      open
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("stock:expiry.removeTitle")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={adjust.isPending} onClick={submit}>
            {t("stock:expiry.remove")}
          </Button>
        </>
      }
    >
      <p>
        {t("stock:expiry.removeBody", {
          name,
          lot: row.lotNumber,
          qty: formatQty(row.qty, lang),
          location: nameOf(row.locationId),
        })}
      </p>
      <p className="text-sm text-ink-2">{t("stock:expiry.removeHint")}</p>
      {adjust.error ? (
        <p role="alert" className="text-danger">
          {errorText(adjust.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
