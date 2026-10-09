import { ArrowLeft } from "lucide-react";
import { type FormEvent, type ReactNode, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate, useParams } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DataTable } from "@/components/data-table/data-table";
import { ExportMenu } from "@/components/data-table/export-menu";
import { LoadMore } from "@/components/data-table/list-notices";
import { DateRangePicker } from "@/components/date-range";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { StatTile } from "@/components/stat-tile";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { StatementA4Actions } from "@/features/customers/StatementA4Actions";
import { ledgerColumns } from "@/features/customers/StatementPanel";
import { type DateRange, isDay, rangeOf, today } from "@/lib/dates";
import { useSession } from "@/lib/ipc/auth";
import {
  useArchiveSupplier,
  useRestoreSupplier,
  useSupplier,
  useSupplierAging,
  useSupplierEntryPages,
  useSupplierOpening,
  useSupplierStatement,
} from "@/lib/ipc/suppliers";
import type { Permission, Supplier } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { InvoiceDialog } from "./InvoiceDialog";
import { PaySupplierDialog } from "./PaySupplierDialog";
import { PurchaseDocuments } from "./PurchasesPage";
import { ReceiveDialog } from "./ReceiveDialog";
import { SupplierForm } from "./SuppliersTab";
import { SupplierReturnDialog } from "./SupplierReturnDialog";

type Tab = "entries" | "payments" | "documents" | "statement";
const TABS: readonly Tab[] = ["entries", "payments", "documents", "statement"];
type Open = "pay" | "receive" | "invoice" | "return" | "edit" | "opening" | null;

/** `/purchases/suppliers/:id`: the supplier's file (PUR-001, PUR-005…010). */
export function SupplierDetail() {
  const { t } = useTranslation();
  const { id = "" } = useParams();
  const supplier = useSupplier(id === "" ? null : id);
  const errorText = useErrorText();
  const back = (
    <Link
      to="/purchases?tab=suppliers"
      className="inline-flex min-h-11 items-center gap-2 font-semibold text-ink-2 hover:text-ink"
    >
      <ArrowLeft aria-hidden className="size-5 rtl:-scale-x-100" />
      {t("purchases:supplier.back")}
    </Link>
  );
  if (!supplier.data) {
    return (
      <div className="flex flex-col gap-4">
        {back}
        {supplier.error ? (
          <p role="alert" className="text-danger">
            {errorText(supplier.error)}
          </p>
        ) : (
          <p className="text-ink-2">{t("common:status.loading")}</p>
        )}
      </div>
    );
  }
  return <SupplierFile key={supplier.data.id} supplier={supplier.data} back={back} />;
}

function SupplierFile({ supplier, back }: { supplier: Supplier; back: ReactNode }) {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const errorText = useErrorText();
  const permissions = useSession().data?.permissions ?? [];
  const can = (p: Permission) => permissions.includes(p);
  const manages = can("suppliers.manage");
  const buys = can("purchases.manage");
  // The ageing and the documents are purchases: whoever only keeps the supplier files (without
  // `purchases.view`) sees the account, not zeros or empty lists the backend would refuse.
  const views = can("purchases.view");
  const tabs = views ? TABS : TABS.filter((k) => k !== "documents");
  const active = !supplier.archived;
  const [tab, setTab] = useState<Tab>("entries");
  const [open, setOpen] = useState<Open>(null);
  // Newest first, whatever the length of the account; older pages on demand.
  const entries = useSupplierEntryPages(supplier.id);
  const aging = useSupplierAging(today(), views).data?.find((row) => row.partyId === supplier.id);
  const archive = useArchiveSupplier();
  const restore = useRestoreSupplier();
  const failed = (error: unknown) => toast.error(errorText(error));
  // From the backend, not from the loaded pages: the opening is the oldest entry.
  const hasOpening = entries.hasOpening ?? true;
  const columns = useMemo(() => ledgerColumns(t, lang), [t, lang]);
  const payments = useMemo(() => entries.entries.filter((e) => e.kind === "payment"), [entries.entries]);
  const close = () => setOpen(null);

  return (
    <div className="flex flex-col gap-6">
      {back}
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h1 className="flex flex-wrap items-center gap-3 text-3xl font-bold">
            {supplier.name}
            {supplier.archived ? <Badge>{t("customers:status.archived")}</Badge> : null}
          </h1>
          <p className="flex flex-wrap items-center gap-x-3 text-ink-2">
            <bdi dir="ltr">{supplier.code}</bdi>
            {supplier.phone ? <bdi dir="ltr">{supplier.phone}</bdi> : null}
            {supplier.address ? <span dir="auto">{supplier.address}</span> : null}
          </p>
          <dl className="flex flex-wrap gap-x-6 gap-y-1 text-sm">
            {supplier.nif ? (
              <div className="flex gap-2">
                <dt className="text-ink-2">{t("customers:fields.nif")}</dt>
                <dd>
                  <bdi dir="ltr">{supplier.nif}</bdi>
                </dd>
              </div>
            ) : null}
            {supplier.rib ? (
              <div className="flex gap-2">
                <dt className="text-ink-2">{t("purchases:supplier.fields.rib")}</dt>
                <dd>
                  <bdi dir="ltr">{supplier.rib}</bdi>
                </dd>
              </div>
            ) : null}
            {supplier.paymentTermsDays !== null ? (
              <div className="flex gap-2">
                <dt className="text-ink-2">{t("purchases:supplier.fields.paymentTerms")}</dt>
                <dd>{t("purchases:supplier.days", { count: supplier.paymentTermsDays })}</dd>
              </div>
            ) : null}
          </dl>
        </div>
        <div className="flex flex-wrap gap-2">
          {manages ? (
            <>
              <Button variant="outline" onClick={() => setOpen("edit")}>
                {t("customers:detail.edit")}
              </Button>
              {active && !hasOpening ? (
                <Button variant="outline" onClick={() => setOpen("opening")}>
                  {t("customers:detail.opening")}
                </Button>
              ) : null}
              {active ? (
                <Button
                  variant="danger"
                  disabled={archive.isPending}
                  onClick={() => archive.mutate(supplier.id, { onError: failed })}
                >
                  {t("customers:detail.archive")}
                </Button>
              ) : (
                <Button
                  variant="outline"
                  disabled={restore.isPending}
                  onClick={() => restore.mutate(supplier.id, { onError: failed })}
                >
                  {t("customers:detail.restore")}
                </Button>
              )}
            </>
          ) : null}
          {buys && active ? (
            <>
              <Button
                variant="secondary"
                onClick={() => void navigate(`/purchases/orders/new?supplier=${supplier.id}`)}
              >
                {t("purchases:order.new")}
              </Button>
              <Button variant="secondary" onClick={() => setOpen("receive")}>
                {t("purchases:receive.new")}
              </Button>
              <Button variant="secondary" onClick={() => setOpen("invoice")}>
                {t("purchases:invoice.new")}
              </Button>
              <Button variant="secondary" onClick={() => setOpen("return")}>
                {t("purchases:return.new")}
              </Button>
            </>
          ) : null}
          {manages && active ? (
            <Button variant="primary" onClick={() => setOpen("pay")}>
              {t("purchases:pay.new")}
            </Button>
          ) : null}
        </div>
      </header>

      {supplier.archived ? (
        <p role="status" className="rounded-control bg-surface-2 p-3 font-semibold text-ink-2">
          {t("purchases:supplier.archived")}
        </p>
      ) : null}

      <div className="grid gap-3 sm:grid-cols-3 lg:grid-cols-6">
        <StatTile
          label={t("purchases:supplier.balance")}
          value={<Amount value={supplier.balance} className={cn(supplier.balance > 0 && "text-danger")} />}
        />
        {views ? (
          <>
            <StatTile label={t("purchases:aging.current")} value={<Amount value={aging?.current ?? 0} />} />
            <StatTile label={t("purchases:aging.days3160")} value={<Amount value={aging?.days3160 ?? 0} />} />
            <StatTile label={t("purchases:aging.days6190")} value={<Amount value={aging?.days6190 ?? 0} />} />
            <StatTile label={t("purchases:aging.over90")} value={<Amount value={aging?.over90 ?? 0} />} />
            <StatTile label={t("purchases:aging.advance")} value={<Amount value={aging?.advance ?? 0} />} />
          </>
        ) : null}
      </div>

      <div role="tablist" className="flex flex-wrap gap-2 border-b border-line pb-2">
        {tabs.map((k) => (
          <button
            key={k}
            type="button"
            role="tab"
            id={`supplier-tab-${k}`}
            aria-selected={tab === k}
            aria-controls={`supplier-panel-${k}`}
            onClick={() => setTab(k)}
            className={cn(
              "min-h-11 rounded-control px-4 font-semibold",
              tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
            )}
          >
            {t(`purchases:supplier.tabs.${k}`)}
          </button>
        ))}
      </div>
      <div role="tabpanel" id={`supplier-panel-${tab}`} aria-labelledby={`supplier-tab-${tab}`}>
        {tab === "entries" || tab === "payments" ? (
          <>
            <DataTable
              ariaLabel={t(`purchases:supplier.tabs.${tab}`)}
              data={tab === "entries" ? entries.entries : payments}
              columns={columns}
              getRowId={(e) => e.id}
              empty={entries.isPending ? t("common:status.loading") : t("purchases:supplier.noEntries")}
            />
            {entries.hasMore ? <LoadMore onClick={entries.loadMore} pending={entries.loadingMore} /> : null}
          </>
        ) : tab === "documents" ? (
          <PurchaseDocuments supplierId={supplier.id} />
        ) : (
          <SupplierStatement supplier={supplier} exports={views} />
        )}
      </div>

      {open === "pay" ? <PaySupplierDialog supplier={supplier} open onClose={close} /> : null}
      {open === "receive" ? <ReceiveDialog supplier={supplier} open onClose={close} /> : null}
      {open === "invoice" ? <InvoiceDialog supplier={supplier} open onClose={close} /> : null}
      {open === "return" ? <SupplierReturnDialog supplier={supplier} open onClose={close} /> : null}
      {open === "edit" ? <SupplierForm supplier={supplier} onClose={close} /> : null}
      {open === "opening" ? <OpeningDialog supplier={supplier} onClose={close} /> : null}
    </div>
  );
}

/** RPT-037 for a supplier: the balance at the start of the period, its debits and credits, and the
 * balance at its end, all from `suppliers_statement`, printed on A4 or saved as a PDF (M3). Its
 * export is the purchases report (`purchases.view`), offered only when `exports`. */
function SupplierStatement({ supplier, exports }: { supplier: Supplier; exports: boolean }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [range, setRange] = useState<DateRange>(() => rangeOf("this_month", today()));
  const complete = isDay(range.from) && isDay(range.to);
  const valid = complete && range.from <= range.to;
  const statement = useSupplierStatement(valid ? supplier.id : null, range.from, range.to);
  const data = valid ? statement.data : undefined;
  const columns = useMemo(() => ledgerColumns(t, lang), [t, lang]);
  return (
    <section className="flex flex-col gap-4" aria-labelledby="supplier-statement-title">
      <h2 id="supplier-statement-title" className="text-xl font-bold">
        {t("customers:statement.title")}
      </h2>
      <DateRangePicker
        value={range}
        onChange={setRange}
        error={complete && !valid ? t("validation:rangeOrder") : undefined}
      />
      {valid && statement.error ? (
        <p role="alert" className="text-danger">
          {errorText(statement.error)}
        </p>
      ) : null}
      {data ? (
        <>
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <StatTile label={t("customers:statement.opening")} value={<Amount value={data.opening} />} />
            <StatTile label={t("customers:statement.debits")} value={<Amount value={data.debits} />} />
            <StatTile label={t("customers:statement.credits")} value={<Amount value={data.credits} />} />
            <StatTile label={t("customers:statement.closing")} value={<Amount value={data.closing} />} />
          </div>
          <DataTable
            ariaLabel={t("customers:statement.title")}
            data={data.entries}
            columns={columns}
            getRowId={(e) => e.id}
            empty={t("customers:statement.empty")}
          />
        </>
      ) : null}
      <StatementA4Actions side="supplier" partyId={supplier.id} from={range.from} to={range.to} />
      {exports ? (
        <>
          <div className="flex justify-end">
            <ExportMenu
              target={
                valid
                  ? {
                      kind: "report",
                      request: {
                        kind: "purchases_by_supplier",
                        from: range.from,
                        to: range.to,
                        supplierId: supplier.id,
                      },
                    }
                  : null
              }
            />
          </div>
          <p className="text-sm text-ink-2">{t("purchases:supplier.statementExportHint")}</p>
        </>
      ) : null}
    </section>
  );
}

type Direction = "owed" | "advance";

/** PUR-007: what the shop owed the supplier (or had paid in advance) when it started with DZBuild
 * POS, entered once; corrections come as documents or payments afterwards. */
function OpeningDialog({ supplier, onClose }: { supplier: Supplier; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const setOpening = useSupplierOpening();
  const [direction, setDirection] = useState<Direction>("owed");
  const [amount, setAmount] = useState("");
  const [invalid, setInvalid] = useState(false);
  const [note, setNote] = useState("");
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const value = parseAmount(amount);
    if (value === null || value <= 0) {
      setInvalid(true);
      return;
    }
    setOpening.mutate(
      {
        supplierId: supplier.id,
        amount: direction === "owed" ? value : -value,
        note: note.trim() === "" ? null : note.trim(),
      },
      {
        onSuccess: () => {
          toast.success(t("customers:opening.done"));
          onClose();
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("purchases:opening.title")}
      description={t("purchases:opening.hint")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            type="submit"
            form="supplier-opening-form"
            variant="primary"
            disabled={setOpening.isPending}
          >
            {t("customers:opening.save")}
          </Button>
        </>
      }
    >
      <form id="supplier-opening-form" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <RadioCards
          legend={t("customers:opening.direction")}
          name="supplier-opening-direction"
          value={direction}
          onChange={setDirection}
          options={[
            { value: "owed", label: t("purchases:opening.owed") },
            { value: "advance", label: t("purchases:opening.advance") },
          ]}
        />
        <Field
          id="supplier-opening-amount"
          label={t("customers:opening.amount")}
          error={invalid ? t("validation:amountFormat") : undefined}
        >
          <NumericInput
            id="supplier-opening-amount"
            value={amount}
            onValueChange={(v) => {
              setAmount(v);
              setInvalid(false);
            }}
          />
        </Field>
        <Field
          id="supplier-opening-note"
          label={t("customers:opening.note")}
          optional={t("customers:form.optional")}
        >
          <Input
            id="supplier-opening-note"
            dir="auto"
            maxLength={200}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </Field>
        {setOpening.error ? (
          <p role="alert" className="text-danger">
            {errorText(setOpening.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
