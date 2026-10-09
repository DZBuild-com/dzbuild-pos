import { Lock, Printer, ShieldAlert } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { DateRangePicker } from "@/components/date-range";
import { EmptyState } from "@/components/empty-state";
import { Button } from "@/components/ui/button";
import { type DateRange, dayToMs, rangeOf, today } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { useTillLink } from "@/lib/ipc/lan";
import { useRegisterEntries } from "@/lib/ipc/pharmacy";
import { useExportPdf } from "@/lib/ipc/print";
import type { ExportDone, ExportTarget, RegisterEntry } from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const col = columnHelper<RegisterEntry>();

/**
 * PRD-052 / VRT-030: the controlled-medicine register of a period, the only screen that shows
 * health data (loi 18-07 art. 18): it needs `pharmacy.register` and says so. Every entry with its
 * number, ticket, medicine, quantity, prescriber, prescription and patient; printed as the paged
 * register (M3's PDF of the `controlled_register` report) and exported with M2's menu. The register
 * is kept on the main PC only: a till reads it there and prints it there.
 */
export function ControlledRegister() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const session = useSession();
  const till = useTillLink();
  const allowed = session.data?.permissions.includes("pharmacy.register") ?? false;
  const [range, setRange] = useState<DateRange>(() => rangeOf("this_month", today()));
  const [search, setSearch] = useState("");
  const entries = useRegisterEntries(range.from, range.to, allowed);
  const pdf = useExportPdf();
  const target: ExportTarget = {
    kind: "report",
    request: { kind: "controlled_register", from: range.from, to: range.to },
  };

  const text = search.trim().toLowerCase();
  const rows = useMemo(
    () =>
      (entries.data ?? []).filter(
        (e) =>
          text === "" ||
          [e.number, e.productName, e.prescriberName, e.patientName, e.documentNumber ?? ""].some((v) =>
            v.toLowerCase().includes(text),
          ),
      ),
    [entries.data, text],
  );
  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("number", {
          header: () => t("pharmacy:register.number"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("businessDate", {
          header: () => t("pharmacy:register.date"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        col.accessor((e) => e.documentNumber ?? "", {
          id: "ticket",
          header: () => t("pharmacy:register.ticket"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("productName", { header: () => t("pharmacy:register.medicine") }),
        col.accessor("qty", {
          header: () => t("pharmacy:register.qty"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{formatQty(c.getValue(), lang)}</bdi>,
        }),
        col.accessor("prescriberName", { header: () => t("pharmacy:register.prescriber") }),
        col.accessor((e) => e.prescriptionNo ?? "", {
          id: "prescription",
          header: () => t("pharmacy:register.prescription"),
          cell: (c) => (
            <bdi dir="ltr">
              {[c.getValue(), formatDate(dayToMs(c.row.original.prescriptionDate), lang)]
                .filter((v) => v !== "")
                .join(" · ")}
            </bdi>
          ),
        }),
        col.accessor("patientName", { header: () => t("pharmacy:register.patient") }),
      ]),
    [t, lang],
  );

  if (!session.data) return null;
  if (!allowed) {
    return (
      <EmptyState icon={Lock} title={t("pharmacy:register.title")} body={t("pharmacy:register.notAllowed")} />
    );
  }
  const print = () =>
    pdf.mutate(
      { target, lang },
      {
        // `null`: the save dialog was cancelled
        onSuccess: (done: ExportDone | null) => {
          if (done) toast.success(t("pharmacy:register.saved", { path: done.path }));
        },
        onError: (error: unknown) => toast.error(errorText(error)),
      },
    );
  return (
    <section className="flex flex-col gap-4" aria-labelledby="register-title">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <h2 id="register-title" className="text-2xl font-bold">
          {t("pharmacy:register.title")}
        </h2>
        {till.isTill ? null : (
          <Button variant="primary" disabled={pdf.isPending} onClick={print}>
            <Printer aria-hidden />
            {t("pharmacy:register.print")}
          </Button>
        )}
      </div>
      <p
        role="note"
        className="flex items-center gap-2 rounded-control bg-warn-soft p-3 font-semibold text-warn"
      >
        <ShieldAlert aria-hidden className="size-5 shrink-0" />
        <span>{t("pharmacy:register.restricted")}</span>
      </p>
      {till.isTill ? (
        <p role="note" className="rounded-control bg-info-soft p-3 text-info">
          {t("pharmacy:register.onMainPc")}
        </p>
      ) : null}
      <DateRangePicker value={range} onChange={setRange} />
      <TableToolbar search={search} onSearch={setSearch} exportTarget={till.isTill ? null : target} />
      {entries.error ? (
        <p role="alert" className="text-danger">
          {errorText(entries.error)}
        </p>
      ) : null}
      <DataTable
        ariaLabel={t("pharmacy:register.title")}
        data={rows}
        columns={columns}
        getRowId={(e) => e.id}
        sortable={false}
        empty={entries.isPending ? t("common:status.loading") : t("pharmacy:register.empty")}
      />
    </section>
  );
}
