import { FileArchive } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { Button } from "@/components/ui/button";
import { isDay } from "@/lib/dates";
import { formatNumber } from "@/lib/format";
import { isLang, LANGS, type Lang } from "@/lib/i18n";
import { useArchiveProgress, useExportArchive } from "@/lib/ipc/fiscal";
import type { ArchiveDone, ArchiveProgress } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** The last full calendar year before `now`, in the webview's local calendar (a default, not a figure). */
export function lastFullYear(now: Date = new Date()): { from: string; to: string } {
  const year = now.getFullYear() - 1;
  return { from: `${year}-01-01`, to: `${year}-12-31` };
}

/** Any document chain, the audit chain, a terminal's closure chain, the versement receipts, a
 * till's journal kept on the main PC or the main PC's chain over those journals that do not
 * verify. */
export function chainsBroken(done: ArchiveDone): boolean {
  const { documents, audit, closures, receipts, tillJournals, tillMergeFirstBroken } = done.chains;
  return (
    documents.some((chain) => chain.firstBrokenSeq !== null) ||
    audit.firstBrokenSeq !== null ||
    closures.some((terminal) => terminal.report.firstBrokenZ !== null) ||
    receipts.firstBrokenNumber !== null ||
    tillJournals.some((till) => till.report.broken > 0) ||
    tillMergeFirstBroken !== null
  );
}

/**
 * The fiscal archive of a period (AUD-010): the Rust save dialog chooses where, the
 * `archive-progress` events feed the bar, and the result shows the file, the manifest's SHA-256,
 * the files with their rows and whether every chain held. A cancelled dialog shows nothing.
 */
export function ArchiveExport() {
  const { t } = useTranslation("fiscal");
  const lang = useLang();
  const id = useId();
  const errorText = useErrorText();
  const [period, setPeriod] = useState(() => lastFullYear());
  const [summaryLang, setSummaryLang] = useState<Lang>(lang);
  const [progress, setProgress] = useState<ArchiveProgress | null>(null);
  const exportArchive = useExportArchive();
  useArchiveProgress(setProgress);
  const valid = isDay(period.from) && isDay(period.to) && period.from <= period.to;

  const onExport = () => {
    setProgress(null);
    exportArchive.mutate({ from: period.from, to: period.to, lang: summaryLang });
  };

  return (
    <Card>
      <h2 className="text-xl font-bold">{t("tabs.archive")}</h2>
      <p className="text-ink-2">{t("archive.intro")}</p>
      <div className="grid gap-4 sm:grid-cols-3">
        <Field id={`${id}-from`} label={t("archive.from")}>
          <Input
            id={`${id}-from`}
            type="date"
            dir="ltr"
            value={period.from}
            aria-invalid={!valid}
            onChange={(event) => setPeriod((p) => ({ ...p, from: event.target.value }))}
          />
        </Field>
        <Field id={`${id}-to`} label={t("archive.to")}>
          <Input
            id={`${id}-to`}
            type="date"
            dir="ltr"
            value={period.to}
            aria-invalid={!valid}
            onChange={(event) => setPeriod((p) => ({ ...p, to: event.target.value }))}
          />
        </Field>
        <Field id={`${id}-lang`} label={t("archive.language")}>
          <NativeSelect
            id={`${id}-lang`}
            value={summaryLang}
            onChange={(event) => {
              if (isLang(event.target.value)) setSummaryLang(event.target.value);
            }}
          >
            {LANGS.map((l) => (
              <option key={l} value={l} lang={l}>
                {t(`common:languages.${l}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
      </div>
      <div>
        <Button variant="primary" disabled={!valid || exportArchive.isPending} onClick={onExport}>
          <FileArchive aria-hidden />
          {t("archive.export")}
        </Button>
      </div>
      {exportArchive.isPending && progress ? <ProgressBar progress={progress} /> : null}
      {exportArchive.isError ? (
        <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
          {errorText(exportArchive.error)}
        </p>
      ) : null}
      {exportArchive.data ? <Done done={exportArchive.data} lang={lang} /> : null}
    </Card>
  );
}

function ProgressBar({ progress }: { progress: ArchiveProgress }) {
  const { t } = useTranslation("fiscal");
  // A percentage for the bar only; the archive's figures are the backend's.
  const percent = progress.total > 0 ? Math.round((progress.done * 100) / progress.total) : 0;
  const stage = t(`archive.progress.${progress.stage}`);
  return (
    <div className="flex flex-col gap-1">
      <div
        role="progressbar"
        aria-label={stage}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent}
        className="h-3 overflow-hidden rounded-full bg-surface-2"
      >
        <div className="h-full bg-brand transition-[inline-size]" style={{ inlineSize: `${percent}%` }} />
      </div>
      <p className="text-sm text-ink-2">{stage}</p>
    </div>
  );
}

function Done({ done, lang }: { done: ArchiveDone; lang: Lang }) {
  const { t } = useTranslation("fiscal");
  const broken = chainsBroken(done);
  return (
    <div className="flex flex-col gap-3 rounded-card border border-line p-4">
      <p role="status" className="font-semibold">
        {t("archive.saved", { path: done.path })}
      </p>
      <dl className="flex flex-col gap-1">
        <dt className="text-sm text-ink-2">{t("archive.manifest")}</dt>
        <dd>
          <bdi dir="ltr" className="font-mono text-sm break-all">
            {done.manifestSha256}
          </bdi>
        </dd>
      </dl>
      <table aria-label={t("archive.files")} className="w-full border-collapse text-sm">
        <thead className="border-b border-line text-ink-2">
          <tr>
            <th className="py-2 text-start font-semibold">{t("archive.files")}</th>
            <th className="py-2 text-end font-semibold">{t("archive.rows")}</th>
            <th className="py-2 text-end font-semibold">{t("archive.bytes")}</th>
          </tr>
        </thead>
        <tbody>
          {done.files.map((file) => (
            <tr key={file.name} className="border-b border-line even:bg-surface-2">
              <td className="py-1.5">
                <bdi dir="ltr">{file.name}</bdi>
              </td>
              <td className="py-1.5 text-end tabular-nums">
                {file.rows === null ? null : <bdi dir="ltr">{formatNumber(file.rows, lang)}</bdi>}
              </td>
              <td className="py-1.5 text-end tabular-nums">
                <bdi dir="ltr">{formatNumber(file.bytes, lang)}</bdi>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <p
        role={broken ? "alert" : undefined}
        className={broken ? "rounded-control bg-danger-soft p-3 text-danger" : "text-ok"}
      >
        {broken ? t("archive.chainsBroken") : t("archive.chainsIntact")}
      </p>
    </div>
  );
}
