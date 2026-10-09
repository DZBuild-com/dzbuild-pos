import { Undo2 } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { formatDate, formatNumber } from "@/lib/format";
import { useImportJobs, useUndoImport } from "@/lib/ipc/imports";
import type { ImportJob, JobStatus } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const STATUS_TONE: Record<JobStatus, "ok" | "danger" | "info" | "neutral"> = {
  running: "info",
  done: "ok",
  failed: "danger",
  undone: "neutral",
};

/** "2 créés, 0 mis à jour, 0 ignorés, 1 refusés" for a job. */
export function useJobSummary(): (job: ImportJob) => string {
  const { t } = useTranslation();
  const lang = useLang();
  return useCallback(
    (job: ImportJob) =>
      t("imports:done.summary", {
        created: formatNumber(job.created, lang),
        updated: formatNumber(job.updated, lang),
        skipped: formatNumber(job.skipped, lang),
        failed: formatNumber(job.failed, lang),
      }),
    [t, lang],
  );
}

/** IMX-004: "Annuler cet import" (the latest import of the day only), after a confirmation: created
 * records are archived, updated ones restored, opening entries reversed by the backend. */
export function UndoImport({ job, onUndone }: { job: ImportJob; onUndone?: (job: ImportJob) => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const undo = useUndoImport();
  const [asking, setAsking] = useState(false);
  const confirm = () =>
    undo.mutate(job.id, {
      onSuccess: (undone) => {
        setAsking(false);
        toast.success(t("imports:done.undone"));
        onUndone?.(undone);
      },
    });
  return (
    <>
      <Button variant="danger" onClick={() => setAsking(true)}>
        <Undo2 aria-hidden />
        {t("imports:done.undo")}
      </Button>
      {asking ? (
        <Dialog
          open
          onOpenChange={(open) => (open ? undefined : setAsking(false))}
          title={t("imports:done.undoTitle")}
          description={t("imports:done.undoBody")}
          footer={
            <>
              <Button variant="outline" onClick={() => setAsking(false)}>
                {t("imports:done.keep")}
              </Button>
              <Button variant="danger" disabled={undo.isPending} onClick={confirm}>
                {t("imports:done.undo")}
              </Button>
            </>
          }
        >
          {undo.error ? (
            <p role="alert" className="text-danger">
              {errorText(undo.error)}
            </p>
          ) : null}
        </Dialog>
      ) : null}
    </>
  );
}

const col = columnHelper<ImportJob>();

/** IMX-004: the latest imports of the kinds the user may import, newest first; the latest of the
 * day can be undone from here too. */
export function ImportJobs() {
  const { t } = useTranslation();
  const lang = useLang();
  const summary = useJobSummary();
  const jobs = useImportJobs();
  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("createdAt", {
          header: () => t("imports:jobs.columns.date"),
          cell: (c) => <bdi dir="ltr">{formatDate(c.getValue(), lang, { withTime: true })}</bdi>,
        }),
        col.accessor((job) => t(`imports:entities.${job.kind}`), {
          id: "entity",
          header: () => t("imports:jobs.columns.entity"),
        }),
        col.accessor("fileName", {
          header: () => t("imports:jobs.columns.file"),
          cell: (c) => <bdi dir="auto">{c.getValue()}</bdi>,
        }),
        col.accessor("status", {
          header: () => t("imports:jobs.columns.status"),
          cell: (c) => (
            <Badge tone={STATUS_TONE[c.getValue()]}>{t(`imports:jobs.status.${c.getValue()}`)}</Badge>
          ),
        }),
        col.accessor((job) => summary(job), {
          id: "result",
          header: () => t("imports:jobs.columns.result"),
          enableSorting: false,
          cell: (c) => <span title={c.getValue()}>{c.getValue()}</span>,
        }),
        col.display({
          id: "actions",
          header: () => t("imports:jobs.columns.actions"),
          cell: ({ row }) => (row.original.undoable ? <UndoImport job={row.original} /> : null),
        }),
      ]),
    [t, lang, summary],
  );
  return (
    <DataTable
      ariaLabel={t("imports:jobs.title")}
      data={jobs.data ?? []}
      columns={columns}
      getRowId={(job) => job.id}
      rowHeight={56}
      empty={jobs.isPending ? t("common:status.loading") : t("imports:jobs.empty")}
    />
  );
}
