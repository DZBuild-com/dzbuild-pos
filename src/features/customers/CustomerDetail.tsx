import { ArrowLeft, Printer } from "lucide-react";
import { type FormEvent, type ReactNode, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useParams } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { type Columns, DataTable, columnHelper } from "@/components/data-table/data-table";
import { LoadMore } from "@/components/data-table/list-notices";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { StatTile } from "@/components/stat-tile";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { OrdersTable } from "@/features/customer-orders/OrdersTable";
import { ContractsTable } from "@/features/instalments/ContractsTable";
import { LoyaltyTab } from "@/features/loyalty/LoyaltyTab";
import { BuyerFlag } from "@/features/orders/BuyerFlag";
import { CustomerPrices } from "@/features/wholesale/CustomerPrices";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import {
  useAdjustCredit,
  useCreditEntryPages,
  useOpenItems,
  useReprintVersement,
  useSetOpening,
} from "@/lib/ipc/credit";
import {
  useArchiveCustomer,
  useCustomer,
  useCustomerDocuments,
  useRestoreCustomer,
  useSetCustomerBlocked,
} from "@/lib/ipc/customers";
import { useModule } from "@/lib/ipc/packs";
import type { Customer, DocumentSummary, LedgerEntry, Permission } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { usePrinterStatus } from "@/lib/printer-status";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { CustomerForm } from "./CustomerForm";
import { PrivacyTab } from "./PrivacyTab";
import { StatementPanel, ledgerColumns } from "./StatementPanel";
import { VersementDialog } from "./VersementDialog";

type Tab =
  | "entries"
  | "openItems"
  | "documents"
  | "statement"
  | "contracts"
  | "orders"
  | "prices"
  | "loyalty"
  | "privacy";

/** The tabs of the packs (M8) and of personal data are named in their own namespace. */
const TAB_LABELS: Partial<Record<Tab, string>> = {
  contracts: "instalments:tabs.contracts",
  orders: "orders:tabs.orders",
  prices: "wholesale:prices.title",
  loyalty: "loyalty:tabs.loyalty",
  privacy: "privacy:tab",
};
type Open = "versement" | "edit" | "opening" | "adjust" | null;

const entryCol = columnHelper<LedgerEntry>();
const docCol = columnHelper<DocumentSummary>();

/** `/customers/:id`: the customer's file (CRM-001, CRD-001…008). */
export function CustomerDetail() {
  const { t } = useTranslation();
  const { id = "" } = useParams();
  const customer = useCustomer(id === "" ? null : id);
  const errorText = useErrorText();
  const back = (
    <Link
      to="/customers"
      className="inline-flex min-h-11 items-center gap-2 font-semibold text-ink-2 hover:text-ink"
    >
      <ArrowLeft aria-hidden className="size-5 rtl:-scale-x-100" />
      {t("customers:back")}
    </Link>
  );
  if (!customer.data) {
    return (
      <div className="flex flex-col gap-4">
        {back}
        {customer.error ? (
          <p role="alert" className="text-danger">
            {errorText(customer.error)}
          </p>
        ) : (
          <p className="text-ink-2">{t("common:status.loading")}</p>
        )}
      </div>
    );
  }
  return <CustomerFile key={customer.data.id} customer={customer.data} back={back} />;
}

function CustomerFile({ customer, back }: { customer: Customer; back: ReactNode }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const permissions = useSession().data?.permissions ?? [];
  const can = (p: Permission) => permissions.includes(p);
  const seesAccount = can("credit.view") || can("credit.collect");
  const manages = can("customers.manage");
  const active = !customer.archived;
  // M8: the customer's instalment contracts and orders, while their module is on
  const instalmentsOn = useModule("instalments");
  const ordersOn = useModule("orders");
  // PRC-003: the prices agreed with a wholesale customer (read with the file's `customers.view`)
  const wholesaleOn = useModule("wholesale");
  // M8 (LOY-002 lite): the card and the points while the loyalty module is on
  const loyaltyOn = useModule("loyalty");
  const tabs: Tab[] = [
    ...(seesAccount ? (["entries", "openItems"] as const) : []),
    "documents",
    ...(can("credit.view") ? (["statement"] as const) : []),
    ...(instalmentsOn && can("credit.view") ? (["contracts"] as const) : []),
    ...(ordersOn && (can("sales.create") || can("documents.quote")) ? (["orders"] as const) : []),
    ...(wholesaleOn ? (["prices"] as const) : []),
    ...(loyaltyOn ? (["loyalty"] as const) : []),
    // PDP-002/004: consents, export and anonymisation (loi 18-07), for every customer file
    "privacy",
  ];
  // The first tab the user may see, until they pick one (the session arrives after the first paint).
  const [chosen, setTab] = useState<Tab | null>(null);
  const tab: Tab = chosen !== null && tabs.includes(chosen) ? chosen : (tabs[0] ?? "documents");
  const [open, setOpen] = useState<Open>(null);
  // Newest first, whatever the length of the notebook; older pages on demand.
  const entries = useCreditEntryPages(seesAccount ? customer.id : null);
  const openItems = useOpenItems(seesAccount && tab === "openItems" ? customer.id : null);
  const documents = useCustomerDocuments(tab === "documents" ? customer.id : null);
  const block = useSetCustomerBlocked();
  const archive = useArchiveCustomer();
  const restore = useRestoreCustomer();
  // From the backend, not from the loaded pages: the opening is the oldest entry.
  const hasOpening = entries.hasOpening ?? true;
  const failed = (error: unknown) => toast.error(errorText(error));
  const canReprint = can("credit.collect") || can("sales.reprint");

  const entryColumns = useMemo<Columns<LedgerEntry>>(
    () => [
      ...ledgerColumns(t, lang),
      ...(canReprint
        ? entryCol.columns([
            entryCol.display({
              id: "reprint",
              header: () => <span className="sr-only">{t("common:actions.reprint")}</span>,
              cell: ({ row }) =>
                row.original.kind === "payment" && row.original.receiptNumber ? (
                  <ReprintButton entryId={row.original.id} number={row.original.receiptNumber} />
                ) : null,
            }),
          ])
        : []),
    ],
    [t, lang, canReprint],
  );
  const openColumns = useMemo(
    () =>
      entryCol.columns([
        ...ledgerColumns(t, lang).slice(0, 4),
        entryCol.accessor("open", {
          header: () => t("customers:detail.columns.open"),
          enableSorting: false,
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} className="font-semibold" />,
        }),
      ]),
    [t, lang],
  );
  const documentColumns = useMemo(
    () =>
      docCol.columns([
        docCol.accessor((d) => d.number ?? "", {
          id: "number",
          header: () => t("documents:columns.number"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        docCol.accessor("docType", {
          header: () => t("documents:columns.type"),
          cell: (c) => t(`documents:types.${c.getValue()}`),
        }),
        docCol.accessor("businessDate", {
          header: () => t("customers:detail.columns.date"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        docCol.accessor("totalDue", {
          header: () => t("documents:columns.total"),
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
            {customer.name}
            {customer.isBlocked ? <Badge tone="danger">{t("customers:status.blocked")}</Badge> : null}
            {customer.archived ? <Badge>{t("customers:status.archived")}</Badge> : null}
          </h1>
          <p className="flex flex-wrap items-center gap-x-3 text-ink-2">
            <bdi dir="ltr">{customer.code}</bdi>
            <span>{t(`customers:types.${customer.customerType}`)}</span>
            {customer.phone ? <bdi dir="ltr">{customer.phone}</bdi> : null}
            {customer.address ? <span dir="auto">{customer.address}</span> : null}
          </p>
          {/* DZB-015: DZBuild's flag on his number, a warning only (nothing while unlinked) */}
          <BuyerFlag phone={customer.phone} />
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
                <>
                  <Button variant="outline" onClick={() => setOpen("adjust")}>
                    {t("customers:detail.adjust")}
                  </Button>
                  <Button
                    variant="outline"
                    disabled={block.isPending}
                    onClick={() =>
                      block.mutate({ id: customer.id, blocked: !customer.isBlocked }, { onError: failed })
                    }
                  >
                    {t(customer.isBlocked ? "customers:detail.unblock" : "customers:detail.block")}
                  </Button>
                  <Button
                    variant="danger"
                    disabled={archive.isPending}
                    onClick={() => archive.mutate(customer.id, { onError: failed })}
                  >
                    {t("customers:detail.archive")}
                  </Button>
                </>
              ) : (
                <Button
                  variant="outline"
                  disabled={restore.isPending}
                  onClick={() => restore.mutate(customer.id, { onError: failed })}
                >
                  {t("customers:detail.restore")}
                </Button>
              )}
            </>
          ) : null}
          {can("credit.collect") && active ? (
            <Button variant="primary" onClick={() => setOpen("versement")}>
              {t("customers:detail.versement")}
            </Button>
          ) : null}
        </div>
      </header>

      {customer.isBlocked && active ? (
        <p role="status" className="rounded-control bg-danger-soft p-3 font-semibold text-danger">
          {t("customers:detail.blocked")}
        </p>
      ) : null}
      {customer.archived ? (
        <p role="status" className="rounded-control bg-surface-2 p-3 font-semibold text-ink-2">
          {t("customers:detail.archived")}
        </p>
      ) : null}

      {can("credit.view") ? (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <StatTile
            label={t("customers:detail.balance")}
            value={
              <Amount
                value={customer.balance}
                className={cn(
                  customer.creditLimit !== null && customer.balance > customer.creditLimit && "text-danger",
                )}
              />
            }
          />
          <StatTile
            label={t("customers:detail.limit")}
            value={
              customer.creditLimit === null ? (
                <span className="text-ink-2">{t("customers:noLimit")}</span>
              ) : (
                <Amount value={customer.creditLimit} />
              )
            }
          />
          <StatTile
            label={t("customers:detail.available")}
            value={
              customer.availableCredit === null ? (
                <span aria-hidden>—</span>
              ) : (
                <Amount value={customer.availableCredit} />
              )
            }
          />
          <StatTile
            label={t("customers:detail.storeCredit")}
            value={<Amount value={customer.storeCredit} />}
          />
        </div>
      ) : null}

      <div role="tablist" className="flex flex-wrap gap-2 border-b border-line pb-2">
        {tabs.map((k) => (
          <button
            key={k}
            type="button"
            role="tab"
            id={`customer-tab-${k}`}
            aria-selected={tab === k}
            aria-controls={`customer-panel-${k}`}
            onClick={() => setTab(k)}
            className={cn(
              "min-h-11 rounded-control px-4 font-semibold",
              tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
            )}
          >
            {t(TAB_LABELS[k] ?? `customers:detail.${k}`)}
          </button>
        ))}
      </div>
      <div role="tabpanel" id={`customer-panel-${tab}`} aria-labelledby={`customer-tab-${tab}`}>
        {tab === "entries" ? (
          <>
            <DataTable
              ariaLabel={t("customers:detail.entries")}
              data={entries.entries}
              columns={entryColumns}
              getRowId={(e) => e.id}
              empty={entries.isPending ? t("common:status.loading") : t("customers:detail.noEntries")}
            />
            {entries.hasMore ? <LoadMore onClick={entries.loadMore} pending={entries.loadingMore} /> : null}
          </>
        ) : tab === "openItems" ? (
          <DataTable
            ariaLabel={t("customers:detail.openItems")}
            data={openItems.data ?? []}
            columns={openColumns}
            getRowId={(e) => e.id}
            empty={openItems.isPending ? t("common:status.loading") : t("customers:detail.noOpenItems")}
          />
        ) : tab === "documents" ? (
          <DataTable
            ariaLabel={t("customers:detail.documents")}
            data={documents.data ?? []}
            columns={documentColumns}
            getRowId={(d) => d.id}
            empty={documents.isPending ? t("common:status.loading") : t("customers:detail.noDocuments")}
          />
        ) : tab === "contracts" ? (
          <ContractsTable filter={{ customerId: customer.id }} />
        ) : tab === "orders" ? (
          <OrdersTable filter={{ customerId: customer.id }} />
        ) : tab === "prices" ? (
          <CustomerPrices customerId={customer.id} />
        ) : tab === "loyalty" ? (
          <LoyaltyTab customerId={customer.id} />
        ) : tab === "privacy" ? (
          <PrivacyTab customerId={customer.id} />
        ) : (
          <StatementPanel customer={customer} />
        )}
      </div>

      {open === "versement" ? (
        <VersementDialog customer={customer} open onClose={() => setOpen(null)} />
      ) : null}
      {open === "edit" ? <CustomerForm customer={customer} onClose={() => setOpen(null)} /> : null}
      {open === "opening" ? <OpeningDialog customer={customer} onClose={() => setOpen(null)} /> : null}
      {open === "adjust" ? <AdjustDialog customer={customer} onClose={() => setOpen(null)} /> : null}
    </div>
  );
}

/** A DUPLICATA of a versement receipt (journaled by the backend); the receipt leaves the printer
 * dot's list of unprinted ones. */
function ReprintButton({ entryId, number }: { entryId: string; number: string }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const reprint = useReprintVersement();
  return (
    <Button
      variant="ghost"
      disabled={reprint.isPending}
      aria-label={`${t("common:actions.reprint")} ${number}`}
      onClick={() =>
        reprint.mutate(
          { entryId, lang: null },
          {
            onSuccess: (outcome) => {
              usePrinterStatus.getState().printed(entryId);
              toast.success(t("customers:detail.reprinted", { printer: outcome.printer }));
            },
            onError: (error) => toast.error(errorText(error)),
          },
        )
      }
    >
      <Printer aria-hidden />
      {t("common:actions.reprint")}
    </Button>
  );
}

type Direction = "debt" | "credit";

/** An amount and whether it adds to the debt or takes from it: typed positive, sent signed. */
function useSignedAmount() {
  const [direction, setDirection] = useState<Direction>("debt");
  const [amount, setAmount] = useState("");
  const [invalid, setInvalid] = useState(false);
  const signed = (): number | null => {
    const value = parseAmount(amount);
    if (value === null || value <= 0) {
      setInvalid(true);
      return null;
    }
    return direction === "debt" ? value : -value;
  };
  return { direction, setDirection, amount, setAmount, invalid, setInvalid, signed };
}

/** CRD-003: the paper notebook's balance, entered once (the backend refuses a second one). */
function OpeningDialog({ customer, onClose }: { customer: Customer; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const setOpening = useSetOpening();
  const form = useSignedAmount();
  const [note, setNote] = useState("");
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const amount = form.signed();
    if (amount === null) return;
    setOpening.mutate(
      { customerId: customer.id, amount, note: note.trim() === "" ? null : note.trim() },
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
      title={t("customers:opening.title")}
      description={t("customers:opening.hint")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="opening-form" variant="primary" disabled={setOpening.isPending}>
            {t("customers:opening.save")}
          </Button>
        </>
      }
    >
      <form id="opening-form" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <RadioCards
          legend={t("customers:opening.direction")}
          name="opening-direction"
          value={form.direction}
          onChange={form.setDirection}
          options={[
            { value: "debt", label: t("customers:opening.owes") },
            { value: "credit", label: t("customers:opening.advance") },
          ]}
        />
        <Field
          id="opening-amount"
          label={t("customers:opening.amount")}
          error={form.invalid ? t("validation:amountFormat") : undefined}
        >
          <NumericInput
            id="opening-amount"
            value={form.amount}
            onValueChange={(v) => {
              form.setAmount(v);
              form.setInvalid(false);
            }}
          />
        </Field>
        <Field id="opening-note" label={t("customers:opening.note")} optional={t("customers:form.optional")}>
          <Input
            id="opening-note"
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

/** A correction of the account with its reason, kept in its history (never an edit). */
function AdjustDialog({ customer, onClose }: { customer: Customer; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const adjust = useAdjustCredit();
  const form = useSignedAmount();
  const [reason, setReason] = useState("");
  const [noReason, setNoReason] = useState(false);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const amount = form.signed();
    if (reason.trim() === "") setNoReason(true);
    if (amount === null || reason.trim() === "") return;
    adjust.mutate(
      { customerId: customer.id, amount, note: reason.trim() },
      {
        onSuccess: () => {
          toast.success(t("customers:adjust.done"));
          onClose();
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("customers:adjust.title")}
      description={t("customers:adjust.hint")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="adjust-form" variant="primary" disabled={adjust.isPending}>
            {t("customers:adjust.save")}
          </Button>
        </>
      }
    >
      <form id="adjust-form" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <RadioCards
          legend={t("customers:adjust.direction")}
          name="adjust-direction"
          value={form.direction}
          onChange={form.setDirection}
          options={[
            { value: "debt", label: t("customers:adjust.increase") },
            { value: "credit", label: t("customers:adjust.decrease") },
          ]}
        />
        <Field
          id="adjust-amount"
          label={t("customers:adjust.amount")}
          error={form.invalid ? t("validation:amountFormat") : undefined}
        >
          <NumericInput
            id="adjust-amount"
            value={form.amount}
            onValueChange={(v) => {
              form.setAmount(v);
              form.setInvalid(false);
            }}
          />
        </Field>
        <Field
          id="adjust-reason"
          label={t("customers:adjust.reason")}
          error={noReason ? t("validation:required") : undefined}
        >
          <Input
            id="adjust-reason"
            dir="auto"
            maxLength={200}
            value={reason}
            aria-invalid={noReason}
            onChange={(e) => {
              setReason(e.target.value);
              setNoReason(false);
            }}
          />
        </Field>
        {adjust.error ? (
          <p role="alert" className="text-danger">
            {errorText(adjust.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
