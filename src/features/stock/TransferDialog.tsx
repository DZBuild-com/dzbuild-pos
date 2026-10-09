import { Plus, X } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { isApprovalCancelled, useApproval } from "@/components/manager-approval";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import {
  ProductPicker,
  SerialList,
  newLineKey,
  refusedLine,
  useLineLabel,
  useTracking,
  useUnitDecimals,
} from "@/features/purchases/purchase-lines";
import { dayToMs } from "@/lib/dates";
import { formatDate, formatNumber } from "@/lib/format";
import { toAppError } from "@/lib/ipc/client";
import { useLots, useTransfer, useTransfers } from "@/lib/ipc/stock-m2";
import type { TransferDone, TransferLine, TransferSummary } from "@/lib/ipc/types";
import { formatQty, parseQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { LocationSelect, useCan, useLocations, useUserName } from "./stock-parts";

interface TransferDraft {
  key: string;
  productId: string;
  name: string;
  unitCode: string;
  qty: string;
  /** Empty: the backend takes the soonest-expiring lots at the origin first (FEFO). */
  lotNumber: string;
  serials: string[];
}

/** The lots of a product held at the origin, soonest expiry first; none chosen = FEFO. */
function LotSelect({
  id,
  label,
  productId,
  locationId,
  value,
  onChange,
}: {
  id: string;
  label: string;
  productId: string;
  locationId: string;
  value: string;
  onChange: (lotNumber: string) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const lots = (useLots(productId).data ?? []).filter((l) => l.locationId === locationId && l.qty > 0);
  return (
    <Field id={id} label={t("stock:transfer.lot")}>
      <NativeSelect
        id={id}
        aria-label={label}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="">{t("stock:transfer.fefo")}</option>
        {lots.map((l) => (
          <option key={l.lot.id} value={l.lot.lotNumber}>
            {l.lot.expiryDate
              ? t("stock:transfer.lotWithExpiry", {
                  lot: l.lot.lotNumber,
                  date: formatDate(dayToMs(l.lot.expiryDate), lang),
                  qty: formatQty(l.qty, lang),
                })
              : t("stock:transfer.lotHere", { lot: l.lot.lotNumber, qty: formatQty(l.qty, lang) })}
          </option>
        ))}
      </NativeSelect>
    </Field>
  );
}

/**
 * STK-005: a `BT…` transfer note moving goods between two locations, validated at once. Lines by
 * scan or search; a lot-tracked product leaves from the chosen lot (or the soonest-expiring first),
 * a serial-tracked one names each piece. Valued at the CMUP by the backend.
 */
export function TransferDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const errorText = useErrorText();
  const label = useLineLabel();
  const decimalsOf = useUnitDecimals();
  const approval = useApproval();
  const transfer = useTransfer();
  const { active, defaultId, nameOf } = useLocations();
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [lines, setLines] = useState<TransferDraft[]>([]);
  const [driver, setDriver] = useState("");
  const [vehicle, setVehicle] = useState("");
  const [note, setNote] = useState("");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [failure, setFailure] = useState<{ field: string | null; message: string } | null>(null);
  const [sentKeys, setSentKeys] = useState<string[]>([]);
  const [done, setDone] = useState<TransferDone | null>(null);
  const tracking = useTracking(lines.map((l) => l.productId));
  const fromId = from || defaultId || "";
  const toId = to || active.find((l) => l.id !== fromId)?.id || "";
  const refusedAt = refusedLine(failure?.field);
  const refusedKey = refusedAt !== null ? sentKeys[refusedAt] : undefined;
  const optional = t("stock:fields.optional");
  const patch = (key: string, change: Partial<TransferDraft>) => {
    setLines((all) => all.map((l) => (l.key === key ? { ...l, ...change } : l)));
    setErrors((all) => ({ ...all, [key]: "" }));
  };
  const trimmed = (value: string) => (value.trim() === "" ? null : value.trim());

  const submit = () => {
    const found: Record<string, string> = {};
    const sent: TransferLine[] = [];
    for (const line of lines) {
      const info = tracking.of(line.productId);
      const qty = parseQty(line.qty, decimalsOf(line.unitCode));
      if (qty === null) {
        found[line.key] = t("validation:qtyFormat");
      } else if (info?.serials && line.serials.length * 1_000 !== qty) {
        found[line.key] = t("stock:transfer.serialsCount", { qty: line.qty });
      } else {
        sent.push({
          productId: line.productId,
          qty,
          lotNumber: info?.lots && line.lotNumber !== "" ? line.lotNumber : null,
          serials: info?.serials ? line.serials : [],
        });
      }
    }
    setErrors(found);
    setFailure(lines.length === 0 ? { field: "lines", message: t("stock:transfer.nothing") } : null);
    if (lines.length === 0 || Object.keys(found).length > 0) return;
    setSentKeys(lines.map((l) => l.key));
    approval
      .run(() =>
        transfer.mutateAsync({
          fromLocationId: fromId,
          toLocationId: toId,
          lines: sent,
          driverName: trimmed(driver),
          vehicle: trimmed(vehicle),
          note: trimmed(note),
        }),
      )
      .then((result) => {
        toast.success(t("stock:transfer.done", { number: result.document.number ?? "" }));
        setDone(result);
      })
      .catch((error: unknown) => {
        if (!isApprovalCancelled(error)) {
          setFailure({ field: toAppError(error).field ?? null, message: errorText(error) });
        }
      });
  };

  if (done) {
    return (
      <Dialog
        open
        onOpenChange={(o) => (o ? undefined : onClose())}
        title={t("stock:transfer.doneTitle", { number: done.document.number ?? "" })}
        footer={
          <Button variant="primary" onClick={onClose}>
            {t("common:actions.done")}
          </Button>
        }
      >
        <p>
          {t("stock:transfer.doneBody", {
            from: nameOf(done.fromLocationId),
            to: nameOf(done.toLocationId),
            n: formatNumber(done.document.lines.length, lang),
          })}
        </p>
        {can("reports.profit") ? (
          <p className="flex flex-wrap items-center gap-2">
            {t("stock:transfer.value")}
            <Amount value={done.document.totalTtc} className="font-semibold" />
          </p>
        ) : null}
      </Dialog>
    );
  }

  return (
    <Dialog
      open
      size="xl"
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("stock:transfer.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={transfer.isPending || approval.pending || !tracking.ready}
            onClick={submit}
          >
            {t("stock:transfer.submit")}
          </Button>
        </>
      }
    >
      <form
        noValidate
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <div className="grid gap-3 sm:grid-cols-2">
          <LocationSelect
            id="transfer-from"
            label={t("stock:transfer.from")}
            value={fromId}
            onChange={(id) => {
              setFrom(id);
              if (id === toId) setTo("");
              // the lots offered are the origin's
              setLines((all) => all.map((l) => ({ ...l, lotNumber: "" })));
            }}
            error={failure?.field === "fromLocationId" ? failure.message : undefined}
          />
          <LocationSelect
            id="transfer-to"
            label={t("stock:transfer.to")}
            value={toId}
            onChange={setTo}
            exclude={fromId}
            error={failure?.field === "toLocationId" ? failure.message : undefined}
          />
        </div>
        <ProductPicker
          autoFocus
          scanAnywhere
          onPick={(p) =>
            setLines((all) => {
              // the same product scanned again adds one to its line
              const same = all.find((l) => l.productId === p.productId);
              if (same) {
                const qty = parseQty(same.qty, decimalsOf(same.unitCode));
                if (qty === null) return all;
                return all.map((l) =>
                  l === same ? { ...l, qty: formatQty(qty + (p.packQty ?? 1_000), lang) } : l,
                );
              }
              return [
                ...all,
                {
                  key: newLineKey(),
                  productId: p.productId,
                  name: p.name,
                  unitCode: p.unitCode,
                  qty: formatQty(p.packQty ?? 1_000, lang),
                  lotNumber: "",
                  serials: [],
                },
              ];
            })
          }
        />
        <section aria-label={t("stock:transfer.lines")} className="flex flex-col gap-3">
          {lines.map((line) => {
            const info = tracking.of(line.productId);
            return (
              <div key={line.key} className="flex flex-col gap-3 rounded-card border border-line p-3">
                <div className="flex items-center justify-between gap-2">
                  <p className="font-semibold">{line.name}</p>
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={t("purchases:lines.removeOf", { name: line.name })}
                    onClick={() => setLines((all) => all.filter((l) => l.key !== line.key))}
                  >
                    <X aria-hidden />
                  </Button>
                </div>
                <div className="grid gap-3 sm:grid-cols-2">
                  <Field
                    id={`transfer-qty-${line.key}`}
                    label={t("stock:transfer.qty")}
                    error={errors[line.key] || undefined}
                  >
                    <NumericInput
                      id={`transfer-qty-${line.key}`}
                      kind="qty"
                      aria-label={label("stock:transfer.qty", line.name)}
                      aria-invalid={Boolean(errors[line.key])}
                      value={line.qty}
                      onValueChange={(qty) => patch(line.key, { qty })}
                    />
                  </Field>
                  {info?.lots ? (
                    <LotSelect
                      id={`transfer-lot-${line.key}`}
                      label={label("stock:transfer.lot", line.name)}
                      productId={line.productId}
                      locationId={fromId}
                      value={line.lotNumber}
                      onChange={(lotNumber) => patch(line.key, { lotNumber })}
                    />
                  ) : null}
                </div>
                {info?.serials ? (
                  <SerialList
                    id={`transfer-serials-${line.key}`}
                    label={label("stock:transfer.serials", line.name)}
                    serials={line.serials}
                    onChange={(serials) => patch(line.key, { serials })}
                  />
                ) : null}
                {refusedKey === line.key && failure ? (
                  <p role="alert" className="text-sm text-danger">
                    {failure.message}
                  </p>
                ) : null}
              </div>
            );
          })}
        </section>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field id="transfer-driver" label={t("stock:transfer.driver")} optional={optional}>
            <Input
              id="transfer-driver"
              dir="auto"
              maxLength={120}
              value={driver}
              onChange={(e) => setDriver(e.target.value)}
            />
          </Field>
          <Field id="transfer-vehicle" label={t("stock:transfer.vehicle")} optional={optional}>
            <Input
              id="transfer-vehicle"
              dir="auto"
              maxLength={60}
              value={vehicle}
              onChange={(e) => setVehicle(e.target.value)}
            />
          </Field>
        </div>
        <Field id="transfer-note" label={t("stock:fields.note")} optional={optional}>
          <Input
            id="transfer-note"
            dir="auto"
            maxLength={500}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </Field>
        {failure &&
        refusedKey === undefined &&
        failure.field !== "fromLocationId" &&
        failure.field !== "toLocationId" ? (
          <p role="alert" className="text-danger">
            {failure.message}
          </p>
        ) : null}
      </form>
      {approval.dialog}
    </Dialog>
  );
}

const col = columnHelper<TransferSummary>();

/** STK-005: the transfer notes, newest first; the value at the CMUP only with `reports.profit`. */
export function TransfersTab() {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const userName = useUserName();
  const { nameOf } = useLocations();
  const transfers = useTransfers();
  const [search, setSearch] = useState("");
  const [creating, setCreating] = useState(false);
  const costs = can("reports.profit");
  const text = search.trim().toLowerCase();
  const rows = useMemo(
    () =>
      (transfers.data ?? []).filter(
        (r) =>
          text === "" ||
          [r.number ?? "", nameOf(r.fromLocationId), nameOf(r.toLocationId)].some((v) =>
            v.toLowerCase().includes(text),
          ),
      ),
    [transfers.data, text, nameOf],
  );
  const columns = useMemo(
    () =>
      col.columns([
        col.accessor((r) => r.number ?? "", {
          id: "number",
          header: () => t("stock:transfer.columns.number"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("businessDate", {
          header: () => t("stock:transfer.columns.date"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        col.accessor((r) => nameOf(r.fromLocationId), {
          id: "from",
          header: () => t("stock:transfer.columns.from"),
        }),
        col.accessor((r) => nameOf(r.toLocationId), {
          id: "to",
          header: () => t("stock:transfer.columns.to"),
        }),
        col.accessor("lines", {
          header: () => t("stock:transfer.columns.lines"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{formatNumber(c.getValue(), lang)}</bdi>,
        }),
        ...(costs
          ? [
              col.accessor("value", {
                header: () => t("stock:transfer.columns.value"),
                meta: { align: "end" },
                cell: (c) => <Amount value={c.getValue()} />,
              }),
            ]
          : []),
        col.accessor((r) => userName(r.userId), {
          id: "user",
          header: () => t("stock:transfer.columns.user"),
        }),
      ]),
    [t, lang, nameOf, userName, costs],
  );
  return (
    <div className="flex flex-col gap-4">
      {can("stock.transfer") ? (
        <Button variant="primary" className="self-end" onClick={() => setCreating(true)}>
          <Plus aria-hidden />
          {t("stock:transfer.new")}
        </Button>
      ) : null}
      <TableToolbar search={search} onSearch={setSearch} exportTarget={null} />
      <DataTable
        ariaLabel={t("stock:transfer.list")}
        data={rows}
        columns={columns}
        getRowId={(r) => r.id}
        empty={transfers.isPending ? t("common:status.loading") : t("stock:transfer.empty")}
      />
      {creating ? <TransferDialog onClose={() => setCreating(false)} /> : null}
    </div>
  );
}
