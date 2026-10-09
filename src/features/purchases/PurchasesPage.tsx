import { Plus, Tag } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TruncatedNotice, capped } from "@/components/data-table/list-notices";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { DateRangePicker } from "@/components/date-range";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { ImportLink } from "@/features/imports/import-entities";
import { PrintLabelsDialog } from "@/features/labels/PrintLabelsDialog";
import { type DateRange, dayToMs, isDay, rangeOf, today } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { usePurchases } from "@/lib/ipc/purchases";
import { useSupplier } from "@/lib/ipc/suppliers";
import type {
  DocStatus,
  DocType,
  ExportTarget,
  OrderStatus,
  Permission,
  PurchaseSummary,
} from "@/lib/ipc/types";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { InvoiceDialog } from "./InvoiceDialog";
import { ReceiveDialog } from "./ReceiveDialog";
import { SupplierSelect } from "./purchase-lines";
import { SuppliersTab } from "./SuppliersTab";

type Tab = "orders" | "receipts" | "invoices" | "suppliers";
const DOC_TABS = {
  orders: "purchase_order",
  receipts: "goods_receipt",
  invoices: "supplier_invoice",
} as const;
const ORDER_STATUSES: readonly OrderStatus[] = ["draft", "sent", "partial", "received", "cancelled"];

const TONES: Record<OrderStatus | DocStatus, "neutral" | "info" | "warn" | "ok" | "danger"> = {
  draft: "neutral",
  sent: "info",
  partial: "warn",
  received: "ok",
  validated: "ok",
  cancelled: "danger",
};

/** Documents listed at most for a period; a shorter period or a supplier finds the others. */
const LIST_LIMIT = 500;

/** An order's receiving status (PUR-002), else the document's own. */
export function StatusTag({ status }: { status: OrderStatus | DocStatus }) {
  const { t } = useTranslation();
  return <Badge tone={TONES[status]}>{t(`purchases:status.${status}`)}</Badge>;
}

const col = columnHelper<PurchaseSummary>();

/**
 * A list of supplier documents (orders, receipts, invoices, or all of a supplier's), filtered by
 * supplier, period and, for orders, status, searched by number or reference. Excel and CSV export
 * the purchases by supplier over the same period (`purchases_by_supplier`). An order opens its page;
 * a validated goods receipt prints the labels of what it received (LBL-006, `products.labels`).
 */
export function PurchaseDocuments({
  docType = null,
  supplierId: fixedSupplier = null,
}: {
  docType?: DocType | null;
  supplierId?: string | null;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const [supplier, setSupplier] = useState("");
  const [status, setStatus] = useState<OrderStatus | "">("");
  const [range, setRange] = useState<DateRange>(() => rangeOf("this_year", today()));
  const [search, setSearch] = useState("");
  const [labelsOf, setLabelsOf] = useState<string | null>(null);
  const canLabel = useSession().data?.permissions.includes("products.labels") ?? false;
  const supplierId = fixedSupplier ?? (supplier === "" ? null : supplier);
  const complete = isDay(range.from) && isDay(range.to);
  const valid = complete && range.from <= range.to;
  // One more than shown: whether the period holds more documents than the list shows.
  const docs = usePurchases({
    docType,
    supplierId,
    from: valid ? range.from : null,
    to: valid ? range.to : null,
    limit: LIST_LIMIT + 1,
  });
  const loaded = capped(docs.data, LIST_LIMIT);
  const orders = docType === "purchase_order";
  const text = search.trim().toLowerCase();
  const rows = useMemo(
    () =>
      loaded.rows.filter(
        (d) =>
          (status === "" || d.orderStatus === status) &&
          (text === "" ||
            [d.number, d.supplierName, d.supplierRef].some((v) => v?.toLowerCase().includes(text))),
      ),
    [loaded.rows, status, text],
  );
  const exportTarget: ExportTarget | null = valid
    ? {
        kind: "report",
        request: { kind: "purchases_by_supplier", from: range.from, to: range.to, supplierId },
      }
    : null;

  const columns = useMemo(
    () =>
      col.columns([
        col.accessor((d) => d.number ?? "", {
          id: "number",
          header: () => t("purchases:columns.number"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        ...(docType === null
          ? [
              col.accessor("docType", {
                header: () => t("purchases:columns.type"),
                cell: (c) => t(`documents:types.${c.getValue()}`),
              }),
            ]
          : []),
        col.accessor("businessDate", {
          header: () => t("purchases:columns.date"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        ...(fixedSupplier === null
          ? [
              col.accessor((d) => d.supplierName ?? "", {
                id: "supplier",
                header: () => t("purchases:columns.supplier"),
              }),
            ]
          : []),
        ...(orders
          ? []
          : [
              col.accessor((d) => d.supplierRef ?? "", {
                id: "supplierRef",
                header: () => t("purchases:columns.supplierRef"),
                enableSorting: false,
                cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
              }),
            ]),
        col.accessor((d) => d.orderStatus ?? d.status, {
          id: "status",
          header: () => t("purchases:columns.status"),
          cell: (c) => <StatusTag status={c.getValue()} />,
        }),
        col.accessor("totalHt", {
          header: () => t("purchases:columns.totalHt"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("taxTotal", {
          header: () => t("purchases:columns.tax"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("totalTtc", {
          header: () => t("purchases:columns.totalTtc"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} className="font-semibold" />,
        }),
        ...(docType === "goods_receipt" && canLabel
          ? [
              col.display({
                id: "labels",
                header: () => <span className="sr-only">{t("labels:title")}</span>,
                cell: ({ row }) =>
                  row.original.status === "validated" ? (
                    <Button
                      variant="outline"
                      aria-label={t("labels:sources.receipt", { number: row.original.number ?? "" })}
                      // a row opens nothing here, but the button keeps its own click and keys
                      onClick={(event) => {
                        event.stopPropagation();
                        setLabelsOf(row.original.id);
                      }}
                      onKeyDown={(event) => event.stopPropagation()}
                    >
                      <Tag aria-hidden />
                      {t("labels:title")}
                    </Button>
                  ) : null,
              }),
            ]
          : []),
      ]),
    [t, lang, docType, fixedSupplier, orders, canLabel],
  );

  return (
    <div className="flex flex-col gap-4">
      <TableToolbar
        search={search}
        onSearch={setSearch}
        exportTarget={exportTarget}
        filters={
          <div className="flex flex-wrap items-end gap-3">
            {fixedSupplier === null ? (
              <SupplierSelect
                id={`purchases-supplier-${docType ?? "all"}`}
                value={supplier}
                onChange={setSupplier}
                allLabel={t("purchases:filters.allSuppliers")}
              />
            ) : null}
            {orders ? (
              <Field id="purchases-status" label={t("purchases:columns.status")}>
                <NativeSelect
                  id="purchases-status"
                  value={status}
                  onChange={(event) => setStatus(ORDER_STATUSES.find((s) => s === event.target.value) ?? "")}
                >
                  <option value="">{t("purchases:filters.allStatuses")}</option>
                  {ORDER_STATUSES.map((s) => (
                    <option key={s} value={s}>
                      {t(`purchases:status.${s}`)}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
            ) : null}
          </div>
        }
      />
      <DateRangePicker
        value={range}
        onChange={setRange}
        error={complete && !valid ? t("validation:rangeOrder") : undefined}
      />
      {loaded.truncated ? <TruncatedNotice rows={LIST_LIMIT} /> : null}
      <DataTable
        ariaLabel={docType ? t(`documents:types.${docType}`) : t("purchases:supplier.tabs.documents")}
        data={rows}
        columns={columns}
        sortable={!loaded.truncated}
        getRowId={(d) => d.id}
        onRowClick={(d) => {
          if (d.docType === "purchase_order") void navigate(`/purchases/orders/${d.id}`);
        }}
        empty={docs.isPending ? t("common:status.loading") : t("purchases:empty")}
      />
      <p className="text-sm text-ink-2">{t("purchases:exportHint")}</p>
      {labelsOf ? (
        <PrintLabelsDialog
          source={{ kind: "goods_receipt", documentId: labelsOf }}
          onClose={() => setLabelsOf(null)}
        />
      ) : null}
    </div>
  );
}

/** Asks which supplier a receipt or an invoice is from, then opens its dialog. */
function WithSupplier({ action, onClose }: { action: "receive" | "invoice"; onClose: () => void }) {
  const { t } = useTranslation();
  const [choice, setChoice] = useState("");
  const [chosen, setChosen] = useState<string | null>(null);
  const [missing, setMissing] = useState(false);
  const supplier = useSupplier(chosen);
  if (chosen !== null && supplier.data) {
    return action === "receive" ? (
      <ReceiveDialog supplier={supplier.data} open onClose={onClose} />
    ) : (
      <InvoiceDialog supplier={supplier.data} open onClose={onClose} />
    );
  }
  return (
    <Dialog
      open
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("purchases:chooseSupplier.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={chosen !== null}
            onClick={() => {
              setMissing(choice === "");
              if (choice !== "") setChosen(choice);
            }}
          >
            {t("common:actions.next")}
          </Button>
        </>
      }
    >
      <SupplierSelect
        id="choose-supplier"
        value={choice}
        onChange={(id) => {
          setChoice(id);
          setMissing(false);
        }}
        allLabel={t("purchases:chooseSupplier.placeholder")}
        error={missing ? t("validation:required") : undefined}
      />
    </Dialog>
  );
}

/** `/purchases`: purchase orders, goods receipts, supplier invoices and the suppliers (PUR-001…010). */
export function PurchasesPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const permissions = useSession().data?.permissions ?? [];
  const can = (p: Permission) => permissions.includes(p);
  const tabs: Tab[] = [
    ...(can("purchases.view") ? (["orders", "receipts", "invoices"] as const) : []),
    ...(can("purchases.view") || can("suppliers.manage") ? (["suppliers"] as const) : []),
  ];
  const requested = params.get("tab");
  const tab: Tab = tabs.find((k) => k === requested) ?? tabs[0] ?? "suppliers";
  const [starting, setStarting] = useState<"receive" | "invoice" | null>(null);
  const buys = can("purchases.manage");

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <h1 className="text-3xl font-bold">{t("purchases:title")}</h1>
        <div className="flex flex-wrap gap-2">
          <ImportLink entities={["suppliers", "supplier_balances"]} />
          {buys && tab === "orders" ? (
            <Button variant="primary" onClick={() => void navigate("/purchases/orders/new")}>
              <Plus aria-hidden />
              {t("purchases:order.new")}
            </Button>
          ) : null}
          {buys && tab === "receipts" ? (
            <Button variant="primary" onClick={() => setStarting("receive")}>
              <Plus aria-hidden />
              {t("purchases:receive.new")}
            </Button>
          ) : null}
          {buys && tab === "invoices" ? (
            <Button variant="primary" onClick={() => setStarting("invoice")}>
              <Plus aria-hidden />
              {t("purchases:invoice.new")}
            </Button>
          ) : null}
        </div>
      </div>
      <div role="tablist" className="flex flex-wrap gap-2 border-b border-line pb-2">
        {tabs.map((k) => (
          <button
            key={k}
            type="button"
            role="tab"
            id={`purchases-tab-${k}`}
            aria-selected={tab === k}
            aria-controls={`purchases-panel-${k}`}
            onClick={() => setParams({ tab: k }, { replace: true })}
            className={cn(
              "min-h-11 rounded-control px-4 font-semibold",
              tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
            )}
          >
            {t(`purchases:tabs.${k}`)}
          </button>
        ))}
      </div>
      <div role="tabpanel" id={`purchases-panel-${tab}`} aria-labelledby={`purchases-tab-${tab}`}>
        {tab === "suppliers" ? <SuppliersTab /> : <PurchaseDocuments key={tab} docType={DOC_TABS[tab]} />}
      </div>
      {starting ? <WithSupplier action={starting} onClose={() => setStarting(null)} /> : null}
    </div>
  );
}
