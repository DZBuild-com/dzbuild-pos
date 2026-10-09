import { ChevronDown, Plus } from "lucide-react";
import { DropdownMenu } from "radix-ui";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { DateRangePicker } from "@/components/date-range";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { CustomerFilter } from "@/features/reports/ReportFilters";
import { type DateRange, dayToMs, isDay } from "@/lib/dates";
import { formatDate, formatNumber, parseDZD } from "@/lib/format";
import { useArchive } from "@/lib/ipc/billing";
import type { ArchiveQuery, ArchiveRow, DocStatus, DocType, ExportTarget, Session } from "@/lib/ipc/types";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { newKinds, visibleKinds } from "./kinds";
import { StateBadge } from "./StateBadge";

const STATUSES: readonly DocStatus[] = ["draft", "validated", "cancelled"];
const SEARCH_ID = "documents-search";

const col = columnHelper<ArchiveRow>();

/** `/` focuses the search, unless the user is typing in a field or a dialog is open. */
function useSlashFocus(id: string): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.ctrlKey || event.altKey || event.metaKey) return;
      const target = event.target instanceof Element ? event.target : null;
      if (target?.closest("input, textarea, select, [contenteditable='true']")) return;
      if (document.querySelector('[role="dialog"], [role="alertdialog"]')) return;
      event.preventDefault();
      document.getElementById(id)?.focus();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [id]);
}

/**
 * DOC-017: the document archive — every document of the kinds the user may see (drafts too),
 * newest first, with its state and the copies made; filtered by kind, status, period, customer,
 * amount and a number or a name; exported as it is filtered (xlsx, csv, PDF). A row opens the
 * document; "New" types an invoice, a quote, a proforma or a delivery note the user may make.
 */
export function DocumentsPage({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const errorText = useErrorText();
  const allowed = useMemo(() => visibleKinds(session.permissions), [session.permissions]);
  const creatable = useMemo(() => newKinds(session.permissions), [session.permissions]);
  const [kinds, setKinds] = useState<DocType[]>([]);
  const [status, setStatus] = useState<DocStatus | "">("");
  const [search, setSearch] = useState("");
  const [customer, setCustomer] = useState("");
  const [range, setRange] = useState<DateRange>({ from: "", to: "" });
  const [minTotal, setMinTotal] = useState("");
  const [maxTotal, setMaxTotal] = useState("");
  const text = useDebounced(search.trim(), 250);
  useSlashFocus(SEARCH_ID);

  const from = isDay(range.from) ? range.from : null;
  const to = isDay(range.to) ? range.to : null;
  const reversed = from !== null && to !== null && from > to;
  const query: ArchiveQuery = {
    kinds,
    status: status === "" ? null : status,
    text: text === "" ? null : text,
    customerId: customer === "" ? null : customer,
    supplierId: null,
    from: reversed ? null : from,
    to: reversed ? null : to,
    minTotal: parseDZD(minTotal),
    maxTotal: parseDZD(maxTotal),
  };
  const archive = useArchive(query);
  const rows = useMemo(() => archive.data?.pages.flatMap((page) => page.rows) ?? [], [archive.data]);
  const exportTarget: ExportTarget = { kind: "documents", query };
  const toggle = (kind: DocType) =>
    setKinds((all) => (all.includes(kind) ? all.filter((k) => k !== kind) : [...all, kind]));

  const columns = useMemo(
    () =>
      col.columns([
        col.accessor((d) => d.number ?? "", {
          id: "number",
          header: () => t("billing:list.columns.number"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("docType", {
          header: () => t("billing:list.columns.kind"),
          cell: (c) => t(`billing:kinds.${c.getValue()}`),
        }),
        col.accessor("businessDate", {
          header: () => t("billing:list.columns.date"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        col.accessor((d) => d.partyName ?? "", {
          id: "party",
          header: () => t("billing:list.columns.party"),
        }),
        col.accessor("totalTtc", {
          header: () => t("billing:list.columns.totalTtc"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("totalDue", {
          header: () => t("billing:list.columns.totalDue"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} className="font-semibold" />,
        }),
        col.accessor((d) => d.state.state, {
          id: "state",
          header: () => t("billing:list.columns.state"),
          cell: ({ row }) => <StateBadge state={row.original.state} />,
        }),
        col.accessor("outputs", {
          header: () => t("billing:list.columns.copies"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{formatNumber(c.getValue(), lang)}</bdi>,
        }),
      ]),
    [t, lang],
  );

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <h1 className="text-3xl font-bold">{t("billing:list.title")}</h1>
        {creatable.length > 0 ? (
          <DropdownMenu.Root>
            <DropdownMenu.Trigger asChild>
              <Button variant="primary">
                <Plus aria-hidden />
                {t("billing:list.new")}
                <ChevronDown aria-hidden />
              </Button>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content
                align="end"
                sideOffset={4}
                className="z-50 flex min-w-56 flex-col rounded-control border border-line bg-surface p-1 text-ink shadow-overlay"
              >
                {creatable.map((kind) => (
                  <DropdownMenu.Item
                    key={kind}
                    onSelect={() => void navigate(`/documents/new/${kind}`)}
                    className="flex min-h-11 cursor-pointer items-center rounded-sm px-3 outline-none data-[highlighted]:bg-brand-soft"
                  >
                    {t(`billing:kinds.${kind}`)}
                  </DropdownMenu.Item>
                ))}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        ) : null}
      </div>
      {allowed.length > 0 ? (
        <div role="group" aria-label={t("billing:list.columns.kind")} className="flex flex-wrap gap-2">
          {[null, ...allowed].map((kind) => {
            const pressed = kind === null ? kinds.length === 0 : kinds.includes(kind);
            return (
              <Button
                key={kind ?? "all"}
                variant="outline"
                aria-pressed={pressed}
                className={cn(pressed && "border-brand bg-brand-soft hover:bg-brand-soft")}
                onClick={() => (kind === null ? setKinds([]) : toggle(kind))}
              >
                {kind === null ? t("billing:list.all") : t(`billing:kinds.${kind}`)}
              </Button>
            );
          })}
        </div>
      ) : null}
      <TableToolbar
        search={search}
        onSearch={setSearch}
        onScan={setSearch}
        searchId={SEARCH_ID}
        searchHint={t("billing:list.search")}
        exportTarget={exportTarget}
        filters={
          <div className="flex flex-wrap items-end gap-3">
            <Field id="documents-status" label={t("billing:list.status")}>
              <NativeSelect
                id="documents-status"
                className="w-auto"
                value={status}
                onChange={(event) => setStatus(STATUSES.find((s) => s === event.target.value) ?? "")}
              >
                <option value="">{t("billing:list.all")}</option>
                {STATUSES.map((s) => (
                  <option key={s} value={s}>
                    {t(`billing:statuses.${s}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            {session.permissions.includes("customers.view") ? (
              <CustomerFilter
                id="documents-customer"
                label={t("billing:list.customer")}
                value={customer}
                onChange={setCustomer}
              />
            ) : null}
            <div className="w-40">
              <Field id="documents-min" label={t("billing:list.minTotal")}>
                <NumericInput id="documents-min" value={minTotal} onValueChange={setMinTotal} />
              </Field>
            </div>
            <div className="w-40">
              <Field id="documents-max" label={t("billing:list.maxTotal")}>
                <NumericInput id="documents-max" value={maxTotal} onValueChange={setMaxTotal} />
              </Field>
            </div>
          </div>
        }
      />
      <fieldset>
        <legend className="sr-only">{t("billing:list.period")}</legend>
        <DateRangePicker
          value={range}
          onChange={setRange}
          error={reversed ? t("validation:rangeOrder") : undefined}
        />
      </fieldset>
      {archive.error ? (
        <p role="alert" className="text-danger">
          {errorText(archive.error)}
        </p>
      ) : null}
      <DataTable
        ariaLabel={t("billing:list.title")}
        data={rows}
        columns={columns}
        // Newest first a page at a time: sorting only what was loaded would mislead.
        sortable={!archive.hasNextPage}
        getRowId={(d) => d.id}
        onRowClick={(d) => void navigate(`/documents/${d.id}`)}
        empty={archive.isPending ? t("common:status.loading") : t("billing:list.empty")}
      />
      {archive.hasNextPage ? (
        <div className="flex justify-center">
          <Button
            variant="outline"
            disabled={archive.isFetchingNextPage}
            onClick={() => void archive.fetchNextPage()}
          >
            {t("billing:list.loadMore")}
          </Button>
        </div>
      ) : null}
    </div>
  );
}
