import { useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, X } from "lucide-react";
import { type ReactNode, useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { StatTile } from "@/components/stat-tile";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { dayToMs } from "@/lib/dates";
import { formatAmountInput, formatDate } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { toAppError } from "@/lib/ipc/client";
import { qk } from "@/lib/ipc/keys";
import {
  useCancelOrder,
  usePurchase,
  usePurchases,
  useRemaining,
  useSaveOrder,
  useValidateOrder,
} from "@/lib/ipc/purchases";
import { useSupplier } from "@/lib/ipc/suppliers";
import type { Document, DocumentLine, OrderInput, PurchaseLineInput, RemainingLine } from "@/lib/ipc/types";
import { formatQty, parseQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { StatusTag } from "./PurchasesPage";
import { ReceiveDialog } from "./ReceiveDialog";
import {
  type LineDraft,
  type LineErrors,
  type Picked,
  ProductPicker,
  SupplierSelect,
  emptyLine,
  keepKeysInField,
  refusedLine,
  toLineInput,
  useUnitDecimals,
} from "./purchase-lines";

type Lang = ReturnType<typeof useLang>;

/** A saved draft line back in the editor (in units: a pack line comes back as its units). */
function fromDocLine(l: DocumentLine, lang: Lang): LineDraft {
  return {
    key: l.id,
    productId: l.productId ?? "",
    name: l.description,
    unitCode: l.unitCode,
    qty: formatQty(l.qty, lang),
    pack: "",
    cost: formatAmountInput(l.unitPrice, lang),
    originLineId: null,
    lotNumber: "",
    expiryDate: "",
    serials: [],
  };
}

/** A line as the table shows it: what was typed, its problems and the backend's total. */
interface Row extends LineDraft {
  errors: LineErrors;
  decimals: number;
  totalHt: number | null;
}

const col = columnHelper<Row>();

/** `/purchases/orders/:id` (`new` for a new order): a draft is edited, a validated order is
 * received. Creating keeps the editor (and what was typed) while the address takes the new id. */
export function OrderEditor() {
  const { id = "new" } = useParams();
  const [created, setCreated] = useState<string | null>(null);
  return <OrderPage key={id === created ? "new" : id} routeId={id} onCreated={setCreated} />;
}

function OrderPage({ routeId, onCreated }: { routeId: string; onCreated: (id: string) => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const navigate = useNavigate();
  const client = useQueryClient();
  const [orderId, setOrderId] = useState<string | null>(routeId === "new" ? null : routeId);
  const order = usePurchase(orderId);
  const back = (
    <Link
      to="/purchases?tab=orders"
      className="inline-flex min-h-11 items-center gap-2 font-semibold text-ink-2 hover:text-ink"
    >
      <ArrowLeft aria-hidden className="size-5 rtl:-scale-x-100" />
      {t("purchases:order.back")}
    </Link>
  );
  if (orderId !== null && !order.data) {
    return (
      <div className="flex flex-col gap-4">
        {back}
        {order.error ? (
          <p role="alert" className="text-danger">
            {errorText(order.error)}
          </p>
        ) : (
          <p className="text-ink-2">{t("common:status.loading")}</p>
        )}
      </div>
    );
  }
  if (order.data && order.data.status !== "draft") return <OrderView order={order.data} back={back} />;
  return (
    <DraftEditor
      saved={order.data ?? null}
      back={back}
      onCreated={(doc) => {
        client.setQueryData(qk.purchase(doc.id), doc);
        setOrderId(doc.id);
        onCreated(doc.id);
        void navigate(`/purchases/orders/${doc.id}`, { replace: true });
      }}
    />
  );
}

/**
 * PUR-002/010: a draft purchase order. The supplier, lines added by scan or search (a carton's
 * barcode brings its pack size), each line's quantity, pack size and cost HT; saved as a draft as
 * often as needed, then validated (`BC…`) or cancelled. The totals are the backend's after each
 * save (the UI sums nothing).
 */
function DraftEditor({
  saved,
  back,
  onCreated,
}: {
  saved: Document | null;
  back: ReactNode;
  onCreated: (doc: Document) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const buys = useSession().data?.permissions.includes("purchases.manage") ?? false;
  const decimalsOf = useUnitDecimals();
  const [supplierId, setSupplierId] = useState(saved?.supplierId ?? params.get("supplier") ?? "");
  const [supplierRef, setSupplierRef] = useState(saved?.sourceRef ?? "");
  const [note, setNote] = useState(saved?.note ?? "");
  const [lines, setLines] = useState<LineDraft[]>(() =>
    (saved?.lines ?? [])
      .filter((l) => l.productId !== null && l.parentLineId === null)
      .map((l) => fromDocLine(l, lang)),
  );
  const [dirty, setDirty] = useState(saved === null);
  const [lineErrors, setLineErrors] = useState<Record<string, LineErrors>>({});
  const [problems, setProblems] = useState<{ supplier?: boolean; noLines?: boolean }>({});
  const [failure, setFailure] = useState<unknown>(null);
  const [sentKeys, setSentKeys] = useState<string[]>([]);
  const saveOrder = useSaveOrder();
  const validateOrder = useValidateOrder();
  const cancelOrder = useCancelOrder();
  const busy = saveOrder.isPending || validateOrder.isPending || cancelOrder.isPending;

  const patch = useCallback((key: string, change: Partial<LineDraft>) => {
    setLines((all) => all.map((l) => (l.key === key ? { ...l, ...change } : l)));
    setLineErrors((all) => ({ ...all, [key]: {} }));
    setDirty(true);
  }, []);
  const remove = useCallback((key: string) => {
    setLines((all) => all.filter((l) => l.key !== key));
    setDirty(true);
  }, []);
  const pick = (p: Picked) => {
    const pack = p.packQty === null ? "" : formatQty(p.packQty, lang);
    setLines((all) => {
      // The same product scanned again adds one more unit (or pack) to its line.
      const same = all.find((l) => l.productId === p.productId && l.pack === pack);
      const qty = same ? parseQty(same.qty, 3) : null;
      if (same && qty !== null) {
        return all.map((l) => (l === same ? { ...l, qty: formatQty(qty + 1_000, lang) } : l));
      }
      return [...all, emptyLine(p, pack)];
    });
    setDirty(true);
  };

  const rows = useMemo<Row[]>(
    () =>
      lines.map((l, i) => ({
        ...l,
        errors: lineErrors[l.key] ?? {},
        decimals: decimalsOf(l.unitCode),
        totalHt: !dirty && saved ? (saved.lines[i]?.totalHt ?? null) : null,
      })),
    [lines, lineErrors, dirty, saved, decimalsOf],
  );

  const columns = useMemo(() => {
    const field = (key: string, name: string) => t("purchases:lineField", { field: t(key), name });
    return col.columns([
      col.accessor("name", {
        header: () => t("purchases:lines.product"),
        cell: ({ row }) => <span className="font-semibold">{row.original.name}</span>,
      }),
      col.display({
        id: "qty",
        header: () => t("purchases:lines.qty"),
        cell: ({ row }) => {
          const r = row.original;
          return (
            <NumericInput
              kind="qty"
              aria-label={field(r.pack !== "" ? "purchases:lines.packs" : "purchases:lines.qty", r.name)}
              aria-invalid={Boolean(r.errors.qty)}
              value={r.qty}
              onKeyDown={keepKeysInField}
              onValueChange={(qty) => patch(r.key, { qty })}
            />
          );
        },
      }),
      col.display({
        id: "pack",
        header: () => t("purchases:lines.pack"),
        cell: ({ row }) => {
          const r = row.original;
          return (
            <NumericInput
              kind="qty"
              aria-label={field("purchases:lines.pack", r.name)}
              aria-invalid={Boolean(r.errors.pack)}
              value={r.pack}
              onKeyDown={keepKeysInField}
              onValueChange={(pack) => patch(r.key, { pack })}
            />
          );
        },
      }),
      col.display({
        id: "cost",
        header: () => t("purchases:lines.cost"),
        cell: ({ row }) => {
          const r = row.original;
          return (
            <NumericInput
              aria-label={field(r.pack !== "" ? "purchases:lines.packCost" : "purchases:lines.cost", r.name)}
              aria-invalid={Boolean(r.errors.cost)}
              value={r.cost}
              onKeyDown={keepKeysInField}
              onValueChange={(cost) => patch(r.key, { cost })}
            />
          );
        },
      }),
      col.accessor("totalHt", {
        header: () => t("purchases:columns.totalHt"),
        enableSorting: false,
        meta: { align: "end" },
        cell: (c) => {
          const total = c.getValue();
          return total === null ? <span aria-hidden>—</span> : <Amount value={total} />;
        },
      }),
      col.display({
        id: "remove",
        header: () => <span className="sr-only">{t("common:actions.remove")}</span>,
        cell: ({ row }) => (
          <Button
            variant="ghost"
            size="icon"
            aria-label={t("purchases:lines.removeOf", { name: row.original.name })}
            onClick={() => remove(row.original.key)}
          >
            <X aria-hidden />
          </Button>
        ),
      }),
    ]);
  }, [t, patch, remove]);

  const refusal = failure ? toAppError(failure) : null;
  const refusedAt = refusedLine(refusal?.field);
  const refusedName = refusedAt !== null ? lines.find((l) => l.key === sentKeys[refusedAt])?.name : undefined;

  /** The order as the backend takes it, or `null` with the problems shown. */
  const build = (): OrderInput | null => {
    const found: Record<string, LineErrors> = {};
    const sent: PurchaseLineInput[] = [];
    for (const line of lines) {
      const result = toLineInput(line, decimalsOf(line.unitCode));
      if (result.input) sent.push(result.input);
      else found[line.key] = result.errors;
    }
    const next = { supplier: supplierId === "", noLines: lines.length === 0 };
    setLineErrors(found);
    setProblems(next);
    if (next.supplier || next.noLines || Object.keys(found).length > 0) return null;
    setSentKeys(lines.map((l) => l.key));
    return {
      supplierId,
      lines: sent,
      supplierRef: supplierRef.trim() === "" ? null : supplierRef.trim(),
      note: note.trim() === "" ? null : note.trim(),
    };
  };
  const save = async (): Promise<Document | null> => {
    const input = build();
    if (!input) return null;
    const doc = await saveOrder.mutateAsync({ id: saved?.id ?? null, input });
    setDirty(false);
    if (!saved) onCreated(doc);
    return doc;
  };
  const run = async (action: "save" | "validate") => {
    setFailure(null);
    try {
      const doc = dirty || !saved ? await save() : saved;
      if (!doc) return;
      if (action === "save") {
        toast.success(t("purchases:order.saved"));
        return;
      }
      const validated = await validateOrder.mutateAsync(doc.id);
      toast.success(t("purchases:order.validated", { number: validated.number ?? "" }));
    } catch (error) {
      setFailure(error);
    }
  };

  const invalidLines = lines.filter((l) => Object.keys(lineErrors[l.key] ?? {}).length > 0);
  return (
    <div className="flex flex-col gap-6">
      {back}
      <header className="flex flex-wrap items-center justify-between gap-4">
        <h1 className="flex flex-wrap items-center gap-3 text-3xl font-bold">
          {saved ? t("purchases:order.draftTitle") : t("purchases:order.newTitle")}
          <StatusTag status="draft" />
        </h1>
        {buys ? (
          <div className="flex flex-wrap gap-2">
            {saved ? (
              <Button
                variant="danger"
                disabled={busy}
                onClick={() =>
                  cancelOrder.mutate(saved.id, {
                    onSuccess: () => {
                      toast.success(t("purchases:order.cancelled"));
                      void navigate("/purchases?tab=orders");
                    },
                    onError: setFailure,
                  })
                }
              >
                {t("purchases:order.cancel")}
              </Button>
            ) : null}
            <Button variant="secondary" disabled={busy} onClick={() => void run("save")}>
              {t("purchases:order.save")}
            </Button>
            <Button variant="primary" disabled={busy} onClick={() => void run("validate")}>
              {t("purchases:order.validate")}
            </Button>
          </div>
        ) : null}
      </header>
      {/* Only who may buy edits a draft; the others read it. */}
      <fieldset disabled={!buys} className="flex min-w-0 flex-col gap-6">
        <div className="grid gap-4 md:grid-cols-3">
          <SupplierSelect
            id="order-supplier"
            value={supplierId}
            onChange={(id) => {
              setSupplierId(id);
              setProblems((p) => ({ ...p, supplier: false }));
              setDirty(true);
            }}
            allLabel={t("purchases:chooseSupplier.placeholder")}
            error={problems.supplier ? t("validation:required") : undefined}
          />
          <Field
            id="order-ref"
            label={t("purchases:order.supplierRef")}
            optional={t("customers:form.optional")}
          >
            <GuardedInput
              id="order-ref"
              dir="ltr"
              maxLength={48}
              value={supplierRef}
              onValueChange={(v) => {
                setSupplierRef(v);
                setDirty(true);
              }}
            />
          </Field>
          <Field id="order-note" label={t("purchases:order.note")} optional={t("customers:form.optional")}>
            <Input
              id="order-note"
              dir="auto"
              maxLength={500}
              value={note}
              onChange={(e) => {
                setNote(e.target.value);
                setDirty(true);
              }}
            />
          </Field>
        </div>
        {/* a scan in a quantity or cost field still adds its product */}
        <ProductPicker autoFocus scanAnywhere onPick={pick} />
        <DataTable
          ariaLabel={t("purchases:order.lines")}
          data={rows}
          columns={columns}
          getRowId={(r) => r.key}
          rowHeight={60}
          empty={t("purchases:order.noLines")}
        />
      </fieldset>
      {problems.noLines ? (
        <p role="alert" className="text-danger">
          {t("purchases:order.noLines")}
        </p>
      ) : null}
      {invalidLines.length > 0 ? (
        <ul role="alert" className="flex flex-col gap-1 text-danger">
          {invalidLines.map((l) => (
            <li key={l.key}>
              {t("purchases:lineField", {
                field: Object.values(lineErrors[l.key] ?? {})
                  .map((key) => t(key, { count: l.serials.length, qty: l.qty }))
                  .join(" "),
                name: l.name,
              })}
            </li>
          ))}
        </ul>
      ) : null}
      {failure ? (
        <p role="alert" className="text-danger">
          {refusedName
            ? t("purchases:lineField", { field: errorText(failure), name: refusedName })
            : errorText(failure)}
        </p>
      ) : null}
      <section className="flex flex-col gap-2" aria-labelledby="order-totals">
        <h2 id="order-totals" className="text-xl font-bold">
          {t("purchases:order.totals")}
        </h2>
        {saved ? (
          <>
            <div className="grid gap-3 sm:grid-cols-3">
              <StatTile label={t("purchases:columns.totalHt")} value={<Amount value={saved.subtotalHt} />} />
              <StatTile label={t("purchases:columns.tax")} value={<Amount value={saved.taxTotal} />} />
              <StatTile label={t("purchases:columns.totalTtc")} value={<Amount value={saved.totalTtc} />} />
            </div>
            {dirty ? <p className="text-sm text-ink-2">{t("purchases:order.unsaved")}</p> : null}
          </>
        ) : (
          <p className="text-ink-2">{t("purchases:order.totalsAfterSave")}</p>
        )}
      </section>
    </div>
  );
}

const remainingCol = columnHelper<RemainingLine>();

/** A validated (or cancelled) order: what each line still awaits, and "Réceptionner". */
function OrderView({ order, back }: { order: Document; back: ReactNode }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const buys = useSession().data?.permissions.includes("purchases.manage") ?? false;
  const supplier = useSupplier(order.supplierId);
  const summary = usePurchases({ docType: "purchase_order", supplierId: order.supplierId }).data?.find(
    (s) => s.id === order.id,
  );
  const remaining = useRemaining(order.id);
  const [receiving, setReceiving] = useState(false);
  const awaited = (remaining.data ?? []).some((r) => r.remaining > 0);
  const columns = useMemo(
    () =>
      remainingCol.columns([
        remainingCol.accessor((r) => r.line.description, {
          id: "product",
          header: () => t("purchases:lines.product"),
          cell: (c) => <span className="font-semibold">{c.getValue()}</span>,
        }),
        remainingCol.accessor((r) => r.line.qty, {
          id: "ordered",
          header: () => t("purchases:order.ordered"),
          meta: { align: "end" },
          cell: (c) => formatQty(c.getValue(), lang),
        }),
        remainingCol.accessor("received", {
          header: () => t("purchases:order.received"),
          meta: { align: "end" },
          cell: (c) => formatQty(c.getValue(), lang),
        }),
        remainingCol.accessor("remaining", {
          header: () => t("purchases:order.remaining"),
          meta: { align: "end" },
          cell: (c) => <span className="font-semibold">{formatQty(c.getValue(), lang)}</span>,
        }),
        remainingCol.accessor((r) => r.line.unitPrice, {
          id: "cost",
          header: () => t("purchases:lines.cost"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        remainingCol.accessor((r) => r.line.totalHt, {
          id: "total",
          header: () => t("purchases:columns.totalHt"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
      ]),
    [t, lang],
  );
  return (
    <div className="flex flex-col gap-6">
      {back}
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h1 className="flex flex-wrap items-center gap-3 text-3xl font-bold">
            <bdi dir="ltr">{order.number}</bdi>
            <StatusTag status={summary?.orderStatus ?? order.status} />
          </h1>
          <p className="flex flex-wrap items-center gap-x-3 text-ink-2">
            <span>{supplier.data?.name}</span>
            <bdi dir="ltr">{formatDate(dayToMs(order.businessDate), lang)}</bdi>
            {order.sourceRef ? (
              <span>
                {t("purchases:columns.supplierRef")} <bdi dir="ltr">{order.sourceRef}</bdi>
              </span>
            ) : null}
          </p>
          {order.note ? <p dir="auto">{order.note}</p> : null}
        </div>
        {buys && order.status === "validated" && awaited && supplier.data && !supplier.data.archived ? (
          <Button variant="primary" onClick={() => setReceiving(true)}>
            {t("purchases:order.receive")}
          </Button>
        ) : null}
      </header>
      {remaining.error ? (
        <p role="alert" className="text-danger">
          {errorText(remaining.error)}
        </p>
      ) : null}
      <DataTable
        ariaLabel={t("purchases:order.lines")}
        data={remaining.data ?? []}
        columns={columns}
        getRowId={(r) => r.line.id}
        empty={remaining.isPending ? t("common:status.loading") : t("purchases:order.noLines")}
      />
      <div className="grid gap-3 sm:grid-cols-3">
        <StatTile label={t("purchases:columns.totalHt")} value={<Amount value={order.subtotalHt} />} />
        <StatTile label={t("purchases:columns.tax")} value={<Amount value={order.taxTotal} />} />
        <StatTile label={t("purchases:columns.totalTtc")} value={<Amount value={order.totalTtc} />} />
      </div>
      {receiving && supplier.data ? (
        <ReceiveDialog supplier={supplier.data} orderId={order.id} open onClose={() => setReceiving(false)} />
      ) : null}
    </div>
  );
}
