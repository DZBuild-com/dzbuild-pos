import { FileUp, RotateCcw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { formatNumber } from "@/lib/format";
import type { ImportJob, ImportProgress } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { UndoImport, useJobSummary } from "./ImportJobs";

/**
 * IMX-006, step 4: "Importer" sends the checked options only (`ready` is false as soon as the
 * mapping differs from the check), the progress of the batches (`import-progress`) while it runs,
 * then the job's figures and "Annuler cet import" while it is the latest import of the day.
 */
export function ApplyStep({
  ready,
  pending,
  onApply,
  progress,
  job,
  onJob,
  onRestart,
  error,
}: {
  ready: boolean;
  pending: boolean;
  onApply: () => void;
  progress: ImportProgress | null;
  job: ImportJob | null;
  /** The job after an undo. */
  onJob: (job: ImportJob) => void;
  onRestart: () => void;
  error: unknown;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const summary = useJobSummary();
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("imports:steps.import")}</h2>
      {job ? (
        <>
          <p className="flex flex-wrap items-center gap-2 text-lg font-semibold">
            <Badge tone={job.status === "done" ? "ok" : job.status === "failed" ? "danger" : "neutral"}>
              {t(`imports:jobs.status.${job.status}`)}
            </Badge>
            <span>{summary(job)}</span>
          </p>
          {job.status === "failed" ? <p className="text-danger">{t("imports:done.failed")}</p> : null}
          {job.status === "undone" ? <p role="status">{t("imports:done.undone")}</p> : null}
          <div className="flex flex-wrap gap-2">
            {job.undoable ? <UndoImport job={job} onUndone={onJob} /> : null}
            <Button variant="outline" onClick={onRestart}>
              <RotateCcw aria-hidden />
              {t("imports:apply.another")}
            </Button>
          </div>
        </>
      ) : (
        <>
          <p className="text-ink-2">{ready ? t("imports:apply.ready") : t("imports:apply.needsCheck")}</p>
          <div>
            <Button variant="primary" size="lg" disabled={!ready || pending} onClick={onApply}>
              <FileUp aria-hidden />
              {t("imports:apply.run")}
            </Button>
          </div>
          {pending ? (
            <div className="flex flex-col gap-1">
              <div
                role="progressbar"
                aria-label={t("imports:apply.progress")}
                aria-valuemin={0}
                aria-valuemax={progress?.total ?? 0}
                aria-valuenow={progress?.done ?? 0}
                className="h-3 overflow-hidden rounded-full bg-surface-2"
              >
                <div
                  className="h-full bg-brand transition-[inline-size]"
                  style={{
                    inlineSize:
                      progress && progress.total > 0 ? `${(progress.done / progress.total) * 100}%` : "0%",
                  }}
                />
              </div>
              {progress ? (
                <p className="text-sm text-ink-2">
                  {t("imports:apply.progressRows", {
                    done: formatNumber(progress.done, lang),
                    total: formatNumber(progress.total, lang),
                  })}
                </p>
              ) : null}
            </div>
          ) : null}
          {error ? (
            <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
              {errorText(error)}
            </p>
          ) : null}
        </>
      )}
    </Card>
  );
}
