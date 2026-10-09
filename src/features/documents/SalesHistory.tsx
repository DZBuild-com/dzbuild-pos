import { createColumnHelper, tableFeatures, useTable } from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { ScanInput } from "@/components/scan-input";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { formatDate } from "@/lib/format";
import { useUserTiles } from "@/lib/ipc/auth";
import { type DocumentFilter, useDocuments } from "@/lib/ipc/documents";
import type { DocType, DocumentSummary, Session } from "@/lib/ipc/types";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { DocumentSheet } from "./DocumentSheet";

const features = tableFeatures({});
const helper = createColumnHelper<typeof features, DocumentSummary>();
const ROW = 40;

/** SAL-056: tickets and return notes, newest first; this terminal's only without sales.view_all. */
export function SalesHistory({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const tiles = useUserTiles();
  const [number, setNumber] = useState("");
  const [date, setDate] = useState("");
  const [docType, setDocType] = useState<DocType | "">("");
  const [open, setOpen] = useState<string | null>(null);
  const prefix = useDebounced(number.trim().toUpperCase(), 200);
  const filter: DocumentFilter = {
    docType: docType === "" ? null : docType,
    terminalId: null,
    cashSessionId: null,
    businessDate: date === "" ? null : date,
    numberPrefix: prefix === "" ? null : prefix,
  };
  const documents = useDocuments(filter);
  const rows = useMemo(() => documents.data?.pages.flat() ?? [], [documents.data]);
  const names = useMemo(() => new Map((tiles.data ?? []).map((u) => [u.id, u.displayName])), [tiles.data]);
  const columns = useMemo(
    () =>
      helper.columns([
        helper.accessor("number", {
          header: () => t("documents:columns.number"),
          cell: (info) => <bdi dir="ltr">{info.getValue()}</bdi>,
        }),
        helper.accessor("validatedAt", {
          header: () => t("documents:columns.date"),
          cell: (info) => {
            const at = info.getValue();
            return at === null ? null : <bdi dir="ltr">{formatDate(at, lang, { withTime: true })}</bdi>;
          },
        }),
        helper.accessor("docType", {
          header: () => t("documents:columns.type"),
          cell: (info) => t(`documents:types.${info.getValue()}`),
        }),
        helper.accessor("userId", {
          header: () => t("documents:columns.cashier"),
          cell: (info) => names.get(info.getValue()) ?? "",
        }),
        helper.accessor("totalDue", {
          header: () => t("documents:columns.total"),
          cell: (info) => <Amount value={info.getValue()} />,
        }),
      ]),
    [lang, names, t],
  );
  const table = useTable({ features, columns, data: rows });
  const scroller = useRef<HTMLDivElement>(null);
  const tableRows = table.getRowModel().rows;
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual (spec §10.4)
  const virtualizer = useVirtualizer({
    count: tableRows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW,
    overscan: 10,
    initialRect: { width: 1000, height: 600 },
  });
  const items = virtualizer.getVirtualItems();
  const top = items[0]?.start ?? 0;
  const bottom = virtualizer.getTotalSize() - (items.at(-1)?.end ?? 0);

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-3xl font-bold">{t("documents:title")}</h1>
      <div className="flex flex-wrap items-end gap-3">
        <Field id="history-number" label={t("documents:filters.number")}>
          <ScanInput
            id="history-number"
            dir="ltr"
            value={number}
            onChange={(e) => setNumber(e.target.value)}
            onScan={setNumber}
          />
        </Field>
        <Field id="history-date" label={t("documents:filters.date")}>
          <Input
            id="history-date"
            type="date"
            dir="ltr"
            value={date}
            onChange={(e) => setDate(e.target.value)}
          />
        </Field>
        <Field id="history-type" label={t("documents:filters.type")}>
          <NativeSelect
            id="history-type"
            value={docType}
            onChange={(e) => setDocType(e.target.value as DocType | "")}
          >
            <option value="">{t("documents:filters.all")}</option>
            <option value="ticket">{t("documents:types.ticket")}</option>
            <option value="return_note">{t("documents:types.return_note")}</option>
          </NativeSelect>
        </Field>
      </div>
      {rows.length === 0 && documents.isSuccess ? <p className="text-ink-2">{t("documents:empty")}</p> : null}
      {documents.error ? (
        <p role="alert" className="text-danger">
          {errorText(documents.error)}
        </p>
      ) : null}
      <div
        ref={scroller}
        className="max-h-[60dvh] overflow-y-auto rounded-card border border-line bg-surface"
      >
        <table className="w-full border-collapse">
          <thead className="sticky top-0 bg-surface text-sm text-ink-2">
            {table.getHeaderGroups().map((group) => (
              <tr key={group.id} className="h-10">
                {group.headers.map((header) => (
                  <th
                    key={header.id}
                    className={
                      header.column.id === "totalDue"
                        ? "px-3 text-end font-semibold"
                        : "px-3 text-start font-semibold"
                    }
                  >
                    {header.isPlaceholder ? null : <table.FlexRender header={header} />}
                  </th>
                ))}
              </tr>
            ))}
          </thead>
          <tbody>
            {top > 0 ? <tr style={{ height: top }} /> : null}
            {items.map((item) => {
              const row = tableRows[item.index];
              if (!row) return null;
              return (
                <tr
                  key={row.id}
                  tabIndex={0}
                  onClick={() => setOpen(row.original.id)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") setOpen(row.original.id);
                  }}
                  className="h-10 cursor-pointer border-t border-line even:bg-surface-2 hover:bg-brand-soft"
                >
                  {row.getAllCells().map((cell) => (
                    <td key={cell.id} className={cell.column.id === "totalDue" ? "px-3 text-end" : "px-3"}>
                      <table.FlexRender cell={cell} />
                    </td>
                  ))}
                </tr>
              );
            })}
            {bottom > 0 ? <tr style={{ height: bottom }} /> : null}
          </tbody>
        </table>
      </div>
      {documents.hasNextPage ? (
        <div>
          <Button
            variant="outline"
            disabled={documents.isFetchingNextPage}
            onClick={() => void documents.fetchNextPage()}
          >
            {t("common:actions.loadMore")}
          </Button>
        </div>
      ) : null}
      {open ? <DocumentSheet id={open} session={session} onClose={() => setOpen(null)} /> : null}
    </div>
  );
}
