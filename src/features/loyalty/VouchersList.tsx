import { Printer } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { type Columns, DataTable, columnHelper } from "@/components/data-table/data-table";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { dayToMs, today } from "@/lib/dates";
import { formatDate, formatDZD } from "@/lib/format";
import { useReprintVoucher, useVouchers } from "@/lib/ipc/loyalty";
import { useReport } from "@/lib/ipc/reports";
import type { ExportTarget, ReportRequest, Voucher, VoucherKind, VoucherStatus } from "@/lib/ipc/types";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const KINDS: readonly VoucherKind[] = ["gift", "exchange", "loyalty"];
const STATUSES: readonly VoucherStatus[] = ["active", "spent", "expired"];
const STATUS_TONES = { active: "ok", spent: "neutral", expired: "warn" } as const;

const col = columnHelper<Voucher>();

/**
 * LOY-001, PAY-010 lite: the gift, exchange and loyalty vouchers (newest first, 500 at most) with
 * their balance and status, filtered by kind, status, code, scanned barcode or buyer; the slip
 * reprinted; and what the shop still owes the holders, the vouchers report's total as of today
 * (the UI adds up no amount).
 */
export function VouchersList({ canReprint }: { canReprint: boolean }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [search, setSearch] = useState("");
  const [kind, setKind] = useState<VoucherKind | null>(null);
  const [status, setStatus] = useState<VoucherStatus | null>(null);
  const text = useDebounced(search.trim(), 200);
  const filter = useMemo(() => ({ kind, status, text }), [kind, status, text]);
  const vouchers = useVouchers(filter);
  const day = today();
  const request = useMemo<ReportRequest>(
    () => ({ kind: "gift_vouchers", from: day, to: day, lang }),
    [day, lang],
  );
  const report = useReport(request);
  const table = report.data?.report.table;
  const balanceAt = table?.columns.findIndex((c) => c.key === "balance") ?? -1;
  const owed = balanceAt >= 0 ? table?.totals?.[balanceAt] : null;
  const exportTarget: ExportTarget = { kind: "report", request };
  const columns = useMemo<Columns<Voucher>>(
    () =>
      col.columns([
        col.accessor("code", {
          header: () => t("loyalty:list.columns.code"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("kind", {
          header: () => t("loyalty:list.columns.kind"),
          cell: (c) => t(`loyalty:kinds.${c.getValue()}`),
        }),
        col.accessor("issuedOn", {
          header: () => t("loyalty:list.columns.issued"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        col.accessor("expiresOn", {
          header: () => t("loyalty:list.columns.expires"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        col.accessor("initialAmount", {
          header: () => t("loyalty:list.columns.initial"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("balance", {
          header: () => t("loyalty:list.columns.balance"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} className="font-semibold" />,
        }),
        col.accessor("status", {
          header: () => t("loyalty:list.columns.status"),
          cell: (c) => {
            const status: VoucherStatus = c.getValue();
            return <Badge tone={STATUS_TONES[status]}>{t(`loyalty:statuses.${status}`)}</Badge>;
          },
        }),
        col.accessor((v) => v.buyerName ?? "", {
          id: "buyer",
          header: () => t("loyalty:list.columns.buyer"),
          cell: (c) => <span dir="auto">{c.getValue()}</span>,
        }),
        ...(canReprint
          ? col.columns([
              col.display({
                id: "reprint",
                header: () => <span className="sr-only">{t("loyalty:list.reprint")}</span>,
                cell: ({ row }) => <ReprintButton code={row.original.code} />,
              }),
            ])
          : []),
      ]),
    [t, lang, canReprint],
  );
  return (
    <div className="flex flex-col gap-4">
      {typeof owed === "number" ? (
        <p role="status" className="text-xl font-bold">
          {t("loyalty:list.outstanding", { amount: formatDZD(owed, lang) })}
        </p>
      ) : null}
      <TableToolbar
        search={search}
        onSearch={setSearch}
        onScan={setSearch}
        searchHint={t("loyalty:list.search")}
        exportTarget={exportTarget}
        filters={
          <>
            <Field id="vouchers-kind" label={t("loyalty:list.kind")}>
              <NativeSelect
                id="vouchers-kind"
                className="w-auto"
                value={kind ?? ""}
                onChange={(event) => setKind(KINDS.find((k) => k === event.target.value) ?? null)}
              >
                <option value="">{t("loyalty:list.allKinds")}</option>
                {KINDS.map((k) => (
                  <option key={k} value={k}>
                    {t(`loyalty:kinds.${k}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            <Field id="vouchers-status" label={t("loyalty:list.status")}>
              <NativeSelect
                id="vouchers-status"
                className="w-auto"
                value={status ?? ""}
                onChange={(event) => setStatus(STATUSES.find((s) => s === event.target.value) ?? null)}
              >
                <option value="">{t("loyalty:list.allStatuses")}</option>
                {STATUSES.map((s) => (
                  <option key={s} value={s}>
                    {t(`loyalty:statuses.${s}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          </>
        }
      />
      {vouchers.error ? (
        <p role="alert" className="text-danger">
          {errorText(vouchers.error)}
        </p>
      ) : (
        <DataTable
          ariaLabel={t("loyalty:list.title")}
          data={vouchers.data ?? []}
          columns={columns}
          getRowId={(v) => v.id}
          empty={vouchers.isPending ? t("common:status.loading") : t("loyalty:list.empty")}
        />
      )}
    </div>
  );
}

/** The voucher's slip again, in this user's language. */
function ReprintButton({ code }: { code: string }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const reprint = useReprintVoucher();
  return (
    <Button
      variant="ghost"
      disabled={reprint.isPending}
      aria-label={`${t("loyalty:list.reprint")} ${code}`}
      onClick={() =>
        reprint.mutate(code, {
          onSuccess: () => toast.success(t("loyalty:list.reprinted", { code })),
          onError: (error) => toast.error(errorText(error)),
        })
      }
    >
      <Printer aria-hidden />
      {t("loyalty:list.reprint")}
    </Button>
  );
}
