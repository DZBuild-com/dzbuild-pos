import { ArrowLeft, Eye, EyeOff, ScanLine, Tag } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { isApprovalCancelled, useApproval } from "@/components/manager-approval";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { StatTile } from "@/components/stat-tile";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { PrintLabelsDialog } from "@/features/labels/PrintLabelsDialog";
import { GridEntryButton } from "@/features/purchases/purchase-lines";
import { formatDate, formatNumber } from "@/lib/format";
import { useCategories } from "@/lib/ipc/catalog";
import { useSerialScans } from "@/lib/ipc/devices";
import { useScannerOptions } from "@/lib/ipc/hardware";
import {
  useCancelCount,
  useCloseCount,
  useCountLines,
  useCountLotLines,
  useScanCount,
  useSetCounted,
} from "@/lib/ipc/stock-m2";
import type {
  CountLineView,
  CountLotLineView,
  CountResult,
  CountStatus,
  StockCount,
  Uncounted,
} from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty, parseQtyOrZero } from "@/lib/numeric";
import { useScanner } from "@/lib/scanner";
import { beep } from "@/lib/sounds";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { CollectorImportButton } from "./CollectorImportButton";
import { CountLotsDialog } from "./CountLotsDialog";
import { useCan, useLocations, useUnitName } from "./stock-parts";

const STATUS_TONES: Record<CountStatus, "info" | "ok" | "neutral"> = {
  open: "info",
  closed: "ok",
  cancelled: "neutral",
};

export function CountStatusTag({ status }: { status: CountStatus }) {
  const { t } = useTranslation();
  return <Badge tone={STATUS_TONES[status]}>{t(`stock:counts.status.${status}`)}</Badge>;
}

/** "Inventaire complet", "Catégorie : Boissons" or "Produits choisis". */
export function useScopeLabel(): (count: StockCount) => string {
  const { t } = useTranslation();
  const lang = useLang();
  const categories = useCategories().data;
  return useCallback(
    (count: StockCount) => {
      if (count.scope !== "category") return t(`stock:counts.scope.${count.scope}`);
      const category = categories?.find((c) => c.id === count.categoryId);
      return category
        ? t("stock:counts.categoryOf", { name: localizedName(category, lang) })
        : t("stock:counts.scope.category");
    },
    [t, lang, categories],
  );
}

/** What the last scan did: the line as counted now, or why the code was refused. */
type LastScan = { ok: true; line: CountLineView } | { ok: false; code: string; message: string };

/** The counted quantity of a line, typed: saved on blur (or Enter) in Set mode. Zero is a count
 * ("none found"); an empty field changes nothing. */
function CountedCell({
  line,
  label,
  onSave,
}: {
  line: CountLineView;
  label: string;
  onSave: (productId: string, qty: number) => void;
}) {
  const lang = useLang();
  const initial = line.countedQty === null ? "" : formatQty(line.countedQty, lang);
  const [text, setText] = useState(initial);
  const [invalid, setInvalid] = useState(false);
  const commit = () => {
    if (text.trim() === "" || text === initial) return;
    const qty = parseQtyOrZero(text, line.unitDecimals);
    setInvalid(qty === null);
    if (qty !== null && qty !== line.countedQty) onSave(line.productId, qty);
  };
  return (
    <NumericInput
      kind="qty"
      aria-label={label}
      aria-invalid={invalid}
      value={text}
      onValueChange={(value) => {
        setText(value);
        setInvalid(false);
      }}
      onBlur={commit}
      onKeyDown={(event) => {
        // arrows, Home and End stay in the field instead of moving the row focus
        event.stopPropagation();
        if (event.key === "Enter") {
          event.preventDefault();
          event.currentTarget.blur();
        }
      }}
      className={cn("h-9 text-end", invalid && "border-danger")}
    />
  );
}

function Difference({ variance }: { variance: number | null }) {
  const lang = useLang();
  if (variance === null) return null;
  const text = `${variance > 0 ? "+" : ""}${formatQty(variance, lang)}`;
  return (
    <bdi
      dir="ltr"
      className={cn("font-semibold", variance < 0 && "text-danger", variance > 0 && "text-info")}
    >
      {text}
    </bdi>
  );
}

const col = columnHelper<CountLineView>();

/**
 * STK-009…011: counting one location. Each scan adds one pack of the code read (`counts_scan`); a
 * typed quantity replaces the count (`counts_set`, Set mode). A blind count hides what is expected;
 * whoever may adjust the stock can show it to reconcile before closing. Closing posts the
 * variances (the uncounted products set to zero or kept), cancelling posts nothing. A data
 * collector's file adds its lines to an open count (STK-012); the counted quantities give their
 * shelf labels (LBL-006).
 */
export function CountScreen({ count, onBack }: { count: StockCount; onBack?: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const errorText = useErrorText();
  const unitName = useUnitName();
  const scopeLabel = useScopeLabel();
  const { nameOf } = useLocations();
  const lines = useCountLines(count.id);
  // STK-004: the lots of the lot-tracked products (a pharmacy counts each lot of a medicine)
  const lotLines = useCountLotLines(count.id);
  const lotsOf = useMemo(() => {
    const byProduct = new Map<string, CountLotLineView[]>();
    for (const lot of lotLines.data ?? []) {
      byProduct.set(lot.productId, [...(byProduct.get(lot.productId) ?? []), lot]);
    }
    return byProduct;
  }, [lotLines.data]);
  const [lotsFor, setLotsFor] = useState<string | null>(null);
  const scanCount = useScanCount();
  const { mutateAsync: saveCounted } = useSetCounted();
  const scanner = useScannerOptions();
  const [search, setSearch] = useState("");
  const [pendingOnly, setPendingOnly] = useState(false);
  const [showExpected, setShowExpected] = useState(false);
  const [lastScan, setLastScan] = useState<LastScan | null>(null);
  const [dialog, setDialog] = useState<"close" | "cancel" | "labels" | null>(null);
  const open = count.status === "open";
  const all = useMemo(() => lines.data ?? [], [lines.data]);
  const counted = all.filter((l) => l.countedQty !== null).length;
  // A blind count keeps its expected quantities hidden while counting; `stock.adjust` may show them
  // (the backend sends them to that permission only).
  const reveal = !count.blind || (can("stock.adjust") && showExpected);

  // One promise per scan: a burst of scans beeps and reports each one (a mutation's own callbacks
  // only follow the latest call).
  const onScan = (code: string) => {
    scanCount
      .mutateAsync({ id: count.id, code })
      .then((line) => {
        beep("ok");
        setLastScan({ ok: true, line });
      })
      .catch((error: unknown) => {
        beep("error");
        setLastScan({ ok: false, code, message: errorText(error) });
      });
  };
  const scanning = open && dialog === null;
  useScanner(onScan, { ...scanner, enabled: scanning });
  // a scanner in USB-COM mode counts the same way (HW-020)
  useSerialScans(onScan, scanning);

  const save = useCallback(
    (productId: string, qty: number) => {
      saveCounted({ id: count.id, productId, qty, mode: "set" }).catch((error: unknown) =>
        toast.error(errorText(error)),
      );
    },
    [saveCounted, count.id, errorText],
  );

  const text = search.trim().toLowerCase();
  const rows = useMemo(
    () =>
      all.filter(
        (l) =>
          (!pendingOnly || l.countedQty === null) &&
          (text === "" ||
            [l.nameFr, l.nameAr, l.nameEn, l.reference ?? "", l.barcode ?? ""].some((v) =>
              v.toLowerCase().includes(text),
            )),
      ),
    [all, pendingOnly, text],
  );

  const columns = useMemo(
    () =>
      col.columns([
        col.accessor((l) => l.reference ?? "", {
          id: "reference",
          header: () => t("stock:counts.reference"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor((l) => localizedName(l, lang), {
          id: "product",
          header: () => t("stock:counts.product"),
        }),
        col.accessor((l) => unitName(l.unitCode), {
          id: "unit",
          header: () => t("stock:counts.unit"),
          enableSorting: false,
        }),
        col.accessor((l) => l.countedQty ?? -1, {
          id: "counted",
          header: () => t("stock:counts.counted"),
          meta: { align: "end" },
          cell: (c) => {
            const line = c.row.original;
            const lots = lotsOf.get(line.productId);
            if (lots) {
              // counted lot by lot: the line is the sum of its lots
              return (
                <Button
                  variant="outline"
                  className="h-9"
                  aria-label={t("stock:counts.lots.open", { name: localizedName(line, lang) })}
                  onClick={() => setLotsFor(line.productId)}
                >
                  {line.countedQty === null ? null : <bdi dir="ltr">{formatQty(line.countedQty, lang)}</bdi>}
                  <span>{t("stock:counts.lots.button", { count: lots.length })}</span>
                </Button>
              );
            }
            if (!open) {
              return line.countedQty === null ? null : (
                <bdi dir="ltr">{formatQty(line.countedQty, lang)}</bdi>
              );
            }
            return (
              <CountedCell
                // a new count from the backend (a scan, another field) replaces what the field shows
                key={`${line.productId}:${line.countedQty ?? ""}`}
                line={line}
                label={t("stock:counts.countedOf", { name: localizedName(line, lang) })}
                onSave={save}
              />
            );
          },
        }),
        ...(reveal
          ? [
              col.accessor((l) => l.expectedQty ?? 0, {
                id: "expected",
                header: () => t("stock:counts.expected"),
                meta: { align: "end" },
                cell: (c) =>
                  c.row.original.expectedQty === null ? null : (
                    <bdi dir="ltr">{formatQty(c.row.original.expectedQty, lang)}</bdi>
                  ),
              }),
              col.accessor((l) => l.variance ?? 0, {
                id: "difference",
                header: () => t("stock:counts.difference"),
                meta: { align: "end" },
                cell: (c) => <Difference variance={c.row.original.variance} />,
              }),
            ]
          : []),
      ]),
    [t, lang, unitName, open, reveal, save, lotsOf],
  );

  const date = formatDate(count.startedAt, lang);
  const location = nameOf(count.locationId);
  return (
    <div className="flex flex-col gap-4">
      {onBack ? (
        <Button variant="ghost" className="self-start" onClick={onBack}>
          <ArrowLeft aria-hidden className="rtl:-scale-x-100" />
          {t("stock:counts.back")}
        </Button>
      ) : null}
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex flex-col gap-2">
          <h2 className="text-2xl font-bold">
            {location ? t("stock:counts.titleAt", { date, location }) : t("stock:counts.title", { date })}
          </h2>
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-ink-2">{scopeLabel(count)}</span>
            {count.blind ? <Badge tone="warn">{t("stock:counts.blindTag")}</Badge> : null}
            <CountStatusTag status={count.status} />
          </div>
        </div>
        <StatTile
          label={t("stock:counts.progressLabel")}
          value={
            <bdi dir="ltr">
              {t("stock:counts.progress", {
                counted: formatNumber(counted, lang),
                total: formatNumber(all.length, lang),
              })}
            </bdi>
          }
        />
      </div>
      {open ? (
        <div className="flex flex-col gap-2 rounded-card border border-line bg-surface p-4">
          <p className="flex items-center gap-2 text-ink-2">
            <ScanLine aria-hidden className="size-5 shrink-0" />
            {t("stock:counts.scanHint")}
          </p>
          {lastScan?.ok ? (
            <p role="status" className="font-semibold text-ok">
              {t("stock:counts.lastScan", {
                name: localizedName(lastScan.line, lang),
                qty: formatQty(lastScan.line.countedQty ?? 0, lang),
                unit: unitName(lastScan.line.unitCode),
              })}
            </p>
          ) : null}
          {lastScan && !lastScan.ok ? (
            <p role="alert" className="font-semibold text-danger">
              {t("stock:counts.scanFailed", { code: lastScan.code, message: lastScan.message })}
            </p>
          ) : null}
        </div>
      ) : (
        <p className="text-ink-2">
          {t(count.status === "closed" ? "stock:counts.closedNote" : "stock:counts.cancelledNote")}
        </p>
      )}
      {(open && can("stock.count")) || (can("products.labels") && counted > 0) ? (
        <div className="flex flex-wrap gap-2">
          {open && can("stock.count") ? <CollectorImportButton countId={count.id} /> : null}
          {open && can("stock.count") ? (
            // VRT-060: a variant model counted per size and colour, one count line per filled cell
            <GridEntryButton onAdd={(entered) => entered.forEach((line) => save(line.variantId, line.qty))} />
          ) : null}
          {can("products.labels") && counted > 0 ? (
            <Button variant="outline" onClick={() => setDialog("labels")}>
              <Tag aria-hidden />
              {t("labels:sources.count")}
            </Button>
          ) : null}
        </div>
      ) : null}
      <TableToolbar
        search={search}
        onSearch={setSearch}
        // the window scanner counts a scan; the search keeps what was typed
        guardScans
        exportTarget={can("stock.count") ? { kind: "count_sheet", countId: count.id } : null}
        filters={
          <div className="flex flex-wrap items-center gap-3">
            <label className="flex min-h-11 items-center gap-3 font-semibold">
              <Checkbox checked={pendingOnly} onCheckedChange={(v) => setPendingOnly(v === true)} />
              {t("stock:counts.uncountedOnly")}
            </label>
            {count.blind && can("stock.adjust") ? (
              <Button variant="outline" onClick={() => setShowExpected((v) => !v)}>
                {showExpected ? <EyeOff aria-hidden /> : <Eye aria-hidden />}
                {t(showExpected ? "stock:counts.hideExpected" : "stock:counts.showExpected")}
              </Button>
            ) : null}
          </div>
        }
      />
      <DataTable
        ariaLabel={t("stock:counts.lines")}
        data={rows}
        columns={columns}
        getRowId={(l) => l.productId}
        empty={lines.isPending ? t("common:status.loading") : undefined}
      />
      {open && can("stock.count") ? (
        <div className="flex flex-wrap justify-end gap-2">
          <Button variant="danger" onClick={() => setDialog("cancel")}>
            {t("stock:counts.cancel")}
          </Button>
          <Button variant="primary" onClick={() => setDialog("close")}>
            {t("stock:counts.close")}
          </Button>
        </div>
      ) : null}
      {dialog === "close" ? (
        <CloseCountDialog
          count={count}
          uncounted={all.length - counted}
          onClose={() => setDialog(null)}
          onDone={() => {
            setDialog(null);
            onBack?.();
          }}
        />
      ) : null}
      {dialog === "labels" ? (
        <PrintLabelsDialog source={{ kind: "count", countId: count.id }} onClose={() => setDialog(null)} />
      ) : null}
      {lotsFor !== null && lotsOf.has(lotsFor) ? (
        <CountLotsDialog
          countId={count.id}
          line={all.find((l) => l.productId === lotsFor) ?? all[0]!}
          lots={lotsOf.get(lotsFor) ?? []}
          open={open}
          reveal={reveal}
          onClose={() => setLotsFor(null)}
        />
      ) : null}
      {dialog === "cancel" ? (
        <CancelCountDialog
          count={count}
          onClose={() => setDialog(null)}
          onDone={() => {
            setDialog(null);
            onBack?.();
          }}
        />
      ) : null}
    </div>
  );
}

/** STK-011: posts the variances. A full count sets what was not counted to zero by default; a
 * partial (cycle) count keeps it. The backend returns how many products moved and, with
 * `reports.profit`, the value of the variance. */
function CloseCountDialog({
  count,
  uncounted,
  onClose,
  onDone,
}: {
  count: StockCount;
  uncounted: number;
  onClose: () => void;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const errorText = useErrorText();
  const approval = useApproval();
  const close = useCloseCount();
  const [policy, setPolicy] = useState<Uncounted>(count.scope === "full" ? "zero" : "ignore");
  const [problem, setProblem] = useState<string | null>(null);
  const [result, setResult] = useState<CountResult | null>(null);

  const submit = () => {
    setProblem(null);
    approval
      .run(() => close.mutateAsync({ id: count.id, uncounted: policy }))
      .then((done) => {
        toast.success(t("stock:counts.closed"));
        setResult(done);
      })
      .catch((error: unknown) => {
        if (!isApprovalCancelled(error)) setProblem(errorText(error));
      });
  };

  if (result) {
    return (
      <Dialog
        open
        onOpenChange={(o) => (o ? undefined : onDone())}
        title={t("stock:counts.closed")}
        footer={
          <Button variant="primary" onClick={onDone}>
            {t("common:actions.done")}
          </Button>
        }
      >
        <p className="font-semibold">
          {t("stock:counts.adjusted", { n: formatNumber(result.adjusted, lang) })}
        </p>
        {can("reports.profit") ? (
          <p className="flex flex-wrap items-center gap-2">
            {t("stock:counts.varianceValue")}
            <Amount value={result.varianceValue} className="font-bold" />
          </p>
        ) : null}
      </Dialog>
    );
  }

  return (
    <Dialog
      open
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("stock:counts.close")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={close.isPending || approval.pending} onClick={submit}>
            {t("stock:counts.closeConfirm")}
          </Button>
        </>
      }
    >
      <p className="font-semibold">
        {t("stock:counts.uncountedCount", { n: formatNumber(uncounted, lang) })}
      </p>
      <RadioCards<Uncounted>
        legend={t("stock:counts.uncounted.legend")}
        name="count-uncounted"
        value={policy}
        onChange={setPolicy}
        options={[
          {
            value: "zero",
            label: t("stock:counts.uncounted.zero"),
            hint: t("stock:counts.uncounted.zeroHint"),
          },
          {
            value: "ignore",
            label: t("stock:counts.uncounted.ignore"),
            hint: t("stock:counts.uncounted.ignoreHint"),
          },
        ]}
      />
      {problem ? (
        <p role="alert" className="text-danger">
          {problem}
        </p>
      ) : null}
      {approval.dialog}
    </Dialog>
  );
}

/** Drops an open count: nothing is posted, the count stays listed as cancelled. */
function CancelCountDialog({
  count,
  onClose,
  onDone,
}: {
  count: StockCount;
  onClose: () => void;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const approval = useApproval();
  const cancel = useCancelCount();
  const [problem, setProblem] = useState<string | null>(null);
  const submit = () => {
    setProblem(null);
    approval
      .run(() => cancel.mutateAsync(count.id))
      .then(() => {
        toast.success(t("stock:counts.cancelled"));
        onDone();
      })
      .catch((error: unknown) => {
        if (!isApprovalCancelled(error)) setProblem(errorText(error));
      });
  };
  return (
    <Dialog
      open
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("stock:counts.cancelTitle")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("stock:counts.keep")}
          </Button>
          <Button variant="danger" disabled={cancel.isPending || approval.pending} onClick={submit}>
            {t("stock:counts.cancel")}
          </Button>
        </>
      }
    >
      <p>{t("stock:counts.cancelBody")}</p>
      {problem ? (
        <p role="alert" className="text-danger">
          {problem}
        </p>
      ) : null}
      {approval.dialog}
    </Dialog>
  );
}
