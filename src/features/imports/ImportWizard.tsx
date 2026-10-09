import { ArrowLeft, Download, FileSpreadsheet } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useNavigate, useParams } from "react-router";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { formatNumber } from "@/lib/format";
import {
  useApplyImport,
  useDiscardImport,
  useDryRun,
  useImportPreview,
  useImportProgress,
  useOpenImportFile,
  useSaveTemplate,
} from "@/lib/ipc/imports";
import type {
  DryRun,
  ImportEntity,
  ImportFile,
  ImportJob,
  ImportOptions,
  ImportProgress,
  Session,
} from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { ApplyStep } from "./ApplyStep";
import { CheckStep } from "./CheckStep";
import { ImportJobs } from "./ImportJobs";
import { importableEntities, isImportEntity } from "./import-entities";
import { MappingStep, missingFields } from "./MappingStep";

type Checked = { options: ImportOptions; dry: DryRun };

const STEPS = ["file", "mapping", "check", "import"] as const;

/** The same options (the webview keeps no other state of a check). */
function isEqual(a: ImportOptions, b: ImportOptions): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/**
 * IMX-001…012: the import wizard of one kind of data. 1 the file, picked by the Rust dialog (the
 * webview never sees a path), and its sheet, or the template in the UI language; 2 the columns
 * (the backend's suggestion, editable); 3 the dry run, which keeps nothing; 4 the import, allowed
 * only with the exact options the last check used (the backend refuses any other), then its
 * figures and the undo. A file left without importing is dropped from the backend.
 */
export function ImportWizard({ entity }: { entity: ImportEntity }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [file, setFile] = useState<ImportFile | null>(null);
  const [sheet, setSheet] = useState(0);
  // The title row the user chose (null: the one the reader found).
  const [headerRow, setHeaderRow] = useState<number | null>(null);
  const [mapping, setMapping] = useState(false);
  // What the user changed in the suggested options of this file and sheet (null: nothing yet).
  const [edited, setEdited] = useState<ImportOptions | null>(null);
  const [checked, setChecked] = useState<Checked | null>(null);
  const [job, setJob] = useState<ImportJob | null>(null);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const openFile = useOpenImportFile();
  const template = useSaveTemplate();
  const preview = useImportPreview(file?.token ?? null, sheet, entity, headerRow);
  const dryRun = useDryRun();
  const apply = useApplyImport();
  const { mutate: discard } = useDiscardImport();
  useImportProgress(setProgress);

  const token = file?.token ?? null;
  useEffect(() => {
    if (token === null) return;
    // Another file, or the wizard closed: the backend no longer needs to hold this one.
    return () => discard(token);
  }, [token, discard]);

  const suggested = useMemo<ImportOptions | null>(
    () =>
      file && preview.data
        ? {
            entity,
            policy: "create_only",
            fileName: file.fileName,
            headerRow: preview.data.headerRow,
            mapping: preview.data.suggested,
          }
        : null,
    [entity, file, preview.data],
  );
  const options = edited ?? suggested;
  const upToDate = checked !== null && options !== null && isEqual(checked.options, options);
  const missing = preview.data && options ? missingFields(preview.data, options) : [];
  const canCheck =
    options !== null && missing.length === 0 && (entity !== "count_lines" || Boolean(options.countId));
  const step = !mapping ? 0 : job ? 3 : upToDate ? 2 : 1;

  const reset = (next: ImportFile | null, nextSheet = 0) => {
    setFile(next);
    setSheet(nextSheet);
    setHeaderRow(null);
    setEdited(null);
    setChecked(null);
    setJob(null);
    setProgress(null);
    dryRun.reset();
    apply.reset();
  };
  const choose = () =>
    openFile.mutate(undefined, {
      // `null`: the dialog was cancelled, the file already chosen stays.
      onSuccess: (opened) => {
        if (opened) reset(opened);
      },
    });
  const saveTemplate = () =>
    template.mutate(
      { entity, lang },
      {
        onSuccess: (path) => {
          if (path) toast.success(t("imports:file.templateSaved"));
        },
      },
    );
  const check = () => {
    if (!file || !options) return;
    const run = options;
    dryRun.mutate(
      { token: file.token, sheet, options: run },
      { onSuccess: (dry) => setChecked({ options: run, dry }) },
    );
  };
  const importNow = () => {
    if (!file || !options || !upToDate) return;
    setProgress(null);
    apply.mutate(
      { token: file.token, sheet, options },
      {
        onSuccess: setJob,
        // The check is spent either way: a new one is needed (it shows the rows already there).
        onError: () => setChecked(null),
      },
    );
  };
  // Another title row: its titles, samples and suggested mapping replace the ones shown (the
  // choices made against the old titles would map the wrong columns).
  const chooseHeaderRow = (row: number) => {
    setHeaderRow(row);
    setEdited(null);
    setChecked(null);
  };
  const restart = () => {
    reset(null);
    setMapping(false);
  };
  const fileError = openFile.error ?? template.error ?? preview.error;

  return (
    <div className="flex flex-col gap-4">
      <ol aria-label={t("imports:steps.label")} className="grid grid-cols-4 gap-2">
        {STEPS.map((key, index) => (
          <li
            key={key}
            aria-current={index === step ? "step" : undefined}
            className={cn(
              "rounded-control border-b-4 px-2 pb-1 text-sm",
              index === step ? "border-brand font-bold text-ink" : "border-line text-ink-2",
              index < step && "border-ok",
            )}
          >
            {t(`imports:steps.${key}`)}
          </li>
        ))}
      </ol>

      {!mapping ? (
        <Card>
          <h2 className="text-xl font-bold">{t("imports:steps.file")}</h2>
          <p className="text-ink-2">{t("imports:file.intro")}</p>
          <div className="flex flex-wrap gap-2">
            <Button variant={file ? "outline" : "primary"} disabled={openFile.isPending} onClick={choose}>
              <FileSpreadsheet aria-hidden />
              {t("imports:file.choose")}
            </Button>
            <Button variant="outline" disabled={template.isPending} onClick={saveTemplate}>
              <Download aria-hidden />
              {t("imports:file.template")}
            </Button>
          </div>
          {file ? (
            <div className="flex flex-col gap-3">
              <dl className="grid gap-x-4 gap-y-1 sm:grid-cols-[max-content_1fr]">
                <dt className="text-ink-2">{t("imports:file.name")}</dt>
                <dd className="font-semibold">
                  <bdi dir="auto">{file.fileName}</bdi>
                </dd>
                {file.encoding ? (
                  <>
                    <dt className="text-ink-2">{t("imports:file.encoding")}</dt>
                    <dd>
                      <bdi dir="ltr">{file.encoding}</bdi>
                    </dd>
                  </>
                ) : null}
                {file.delimiter ? (
                  <>
                    <dt className="text-ink-2">{t("imports:file.delimiter")}</dt>
                    <dd>
                      <bdi dir="ltr">{file.delimiter}</bdi>
                    </dd>
                  </>
                ) : null}
              </dl>
              {file.sheets.length > 1 ? (
                <Field id="import-sheet" label={t("imports:file.sheet")}>
                  <NativeSelect
                    id="import-sheet"
                    value={sheet}
                    onChange={(event) => reset(file, Number(event.target.value))}
                  >
                    {file.sheets.map((s, index) => (
                      <option key={index} value={index}>
                        {t("imports:file.sheetOption", { name: s.name, rows: formatNumber(s.rows, lang) })}
                      </option>
                    ))}
                  </NativeSelect>
                </Field>
              ) : null}
              {preview.isPending ? <p className="text-ink-2">{t("imports:file.reading")}</p> : null}
            </div>
          ) : null}
          {fileError ? (
            <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
              {errorText(fileError)}
            </p>
          ) : null}
          {options ? (
            <div className="flex justify-end">
              <Button variant="primary" onClick={() => setMapping(true)}>
                {t("common:actions.next")}
              </Button>
            </div>
          ) : null}
        </Card>
      ) : (
        <>
          <div className="flex flex-wrap items-center justify-between gap-2">
            {job ? null : (
              <Button variant="ghost" onClick={() => setMapping(false)}>
                <ArrowLeft aria-hidden className="rtl:-scale-x-100" />
                {t("common:actions.back")}
              </Button>
            )}
            {file ? (
              <p className="font-semibold">
                <bdi dir="auto">{file.fileName}</bdi>
              </p>
            ) : null}
          </div>
          {!job && preview.data && options ? (
            <>
              <MappingStep
                preview={preview.data}
                options={options}
                onChange={setEdited}
                onHeaderRow={chooseHeaderRow}
              />
              <CheckStep
                entity={entity}
                fields={preview.data.fields}
                policy={options.policy}
                dry={upToDate && checked ? checked.dry : null}
                stale={checked !== null && !upToDate}
                canCheck={canCheck}
                pending={dryRun.isPending}
                onCheck={check}
                error={dryRun.error}
              />
            </>
          ) : null}
          <ApplyStep
            ready={upToDate}
            pending={apply.isPending}
            onApply={importNow}
            progress={progress}
            job={job}
            onJob={setJob}
            onRestart={restart}
            error={apply.error}
          />
        </>
      )}
    </div>
  );
}

/**
 * `/imports/:entity`: the wizard of one import the user may run, a choice of the others, and the
 * history of the latest imports with the undo of the day's last one.
 */
export function ImportPage({ session }: { session: Session }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { entity } = useParams();
  const allowed = importableEntities((p) => session.permissions.includes(p));
  if (!isImportEntity(entity) || !allowed.includes(entity)) {
    return allowed[0] ? <Navigate to={`/imports/${allowed[0]}`} replace /> : <Navigate to="/" replace />;
  }
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <h1 className="text-3xl font-bold">{t("imports:title")}</h1>
        <Field id="import-entity" label={t("imports:entity")}>
          <NativeSelect
            id="import-entity"
            className="w-auto"
            value={entity}
            onChange={(event) => void navigate(`/imports/${event.target.value}`)}
          >
            {allowed.map((e) => (
              <option key={e} value={e}>
                {t(`imports:entities.${e}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
      </div>
      {/* another kind of data starts a new wizard (and drops the file of the previous one) */}
      <ImportWizard key={entity} entity={entity} />
      <section className="flex flex-col gap-3">
        <h2 className="text-xl font-bold">{t("imports:jobs.title")}</h2>
        <ImportJobs />
      </section>
    </div>
  );
}
