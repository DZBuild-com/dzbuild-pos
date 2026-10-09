import { FileSpreadsheet } from "lucide-react";
import { useEffect, useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { formatNumber } from "@/lib/format";
import { useDiscardImport, useImportPreview, useOpenImportFile } from "@/lib/ipc/imports";
import { useChifaApply, useChifaCheck } from "@/lib/ipc/pharmacy";
import type { ChifaApplied, ChifaCheck, ChifaMapping, ChifaProblem, ImportFile } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** The four fields of the list and the words a column title usually carries for each. */
const FIELDS = [
  { key: "code", label: "pharmacy:chifa.code", guess: /code|barre|r[ée]f/i },
  { key: "name", label: "pharmacy:chifa.name", guess: /d[ée]sign|nom|libell|name/i },
  { key: "referencePrice", label: "pharmacy:chifa.price", guess: /tarif|prix|price/i },
  { key: "reimbursable", label: "pharmacy:chifa.reimbursable", guess: /rembours|reimburs/i },
] as const;
type FieldKey = (typeof FIELDS)[number]["key"];

/** The rows of the file shown under the mapping. */
const PREVIEW_ROWS = 5;
/** The problems listed (the count says how many there are). */
const SHOWN_PROBLEMS = 50;

/** What each column most likely holds: a matching title, else the list's usual order. */
export function guessMapping(headers: readonly string[]): ChifaMapping {
  const taken = new Set<number>();
  const find = (pattern: RegExp, fallback: number | null): number | null => {
    const found = headers.findIndex((h, i) => !taken.has(i) && pattern.test(h));
    const index = found >= 0 ? found : fallback !== null && !taken.has(fallback) ? fallback : null;
    if (index !== null) taken.add(index);
    return index;
  };
  return {
    code: find(FIELDS[0].guess, 0) ?? 0,
    name: find(FIELDS[1].guess, 1) ?? 1,
    referencePrice: find(FIELDS[2].guess, 2) ?? 2,
    reimbursable: find(FIELDS[3].guess, headers.length === 0 || headers.length > 3 ? 3 : null),
  };
}

function sameMapping(a: ChifaMapping, b: ChifaMapping): boolean {
  return (
    a.code === b.code &&
    a.name === b.name &&
    a.referencePrice === b.referencePrice &&
    a.reimbursable === b.reimbursable
  );
}

/** The `pharmacy:chifa.reasons.*` word of a refused cell (the backend's column and code). */
function reasonKey({ column, code }: ChifaProblem): string {
  if (column === "code") return code === "too_long" ? "codeLong" : "code";
  if (column === "referencePrice") {
    if (code === "required") return "missing";
    return code === "negative" ? "negative" : "amount";
  }
  return column === "reimbursable" ? "reimbursable" : "other";
}

function columnKey(column: string): string {
  return FIELDS.find((f) => f.key === column)?.label ?? "pharmacy:chifa.code";
}

/**
 * VRT-030 / PRD-051: the Chifa reference list. The file is opened by M2's import dialog (the
 * webview holds its token, never its path), its four columns are mapped with a look at the first
 * rows, "Vérifier" runs the backend's dry run (rows, valid, matched, every problem by row and
 * column), and "Appliquer" writes the list only once the very mapping checked has no problem: the
 * list applies whole or not at all.
 */
export function ChifaImport({ initialFile = null }: { initialFile?: ImportFile | null }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const id = useId();
  const [file, setFile] = useState<ImportFile | null>(initialFile);
  const [edited, setEdited] = useState<ChifaMapping | null>(null);
  const [checked, setChecked] = useState<{ mapping: ChifaMapping; result: ChifaCheck } | null>(null);
  const [applied, setApplied] = useState<ChifaApplied | null>(null);
  const openFile = useOpenImportFile();
  const preview = useImportPreview(file?.token ?? null, 0, "products", null);
  const check = useChifaCheck();
  const apply = useChifaApply();
  const { mutate: discard } = useDiscardImport();

  const token = file?.token ?? null;
  useEffect(() => {
    if (token === null) return;
    // Another file, or the screen closed: the backend no longer needs to hold this one.
    return () => discard(token);
  }, [token, discard]);

  const headers = useMemo(() => preview.data?.headers ?? [], [preview.data]);
  const suggested = useMemo(() => guessMapping(headers), [headers]);
  const mapping = edited ?? suggested;
  const columnCount = Math.max(headers.length, 4);
  const upToDate = checked !== null && sameMapping(checked.mapping, mapping);
  const clean = upToDate && checked.result.problems.length === 0;

  const choose = () =>
    openFile.mutate(undefined, {
      // `null`: the dialog was cancelled, the file already chosen stays
      onSuccess: (opened) => {
        if (!opened) return;
        setFile(opened);
        setEdited(null);
        setChecked(null);
        setApplied(null);
        check.reset();
        apply.reset();
      },
    });
  const setColumn = (key: FieldKey, value: string) => {
    const index = value === "" ? null : Number(value);
    const next: ChifaMapping = { ...mapping };
    if (key === "reimbursable") next.reimbursable = index;
    else if (index !== null) next[key] = index;
    setEdited(next);
  };
  const runCheck = () => {
    if (!file) return;
    const asked = mapping;
    check.mutate(
      { token: file.token, mapping: asked },
      { onSuccess: (result) => setChecked({ mapping: asked, result }) },
    );
  };
  const runApply = () => {
    if (!file || !clean) return;
    apply.mutate(
      { token: file.token, mapping },
      {
        onSuccess: (done) => {
          setApplied(done);
          // the backend dropped the file once the list was written
          setFile(null);
          setChecked(null);
        },
      },
    );
  };

  const shown = (preview.data?.sample ?? []).slice(0, PREVIEW_ROWS);
  const cellOf = (row: string[], index: number | null) => (index === null ? "" : (row[index] ?? ""));
  return (
    <section className="flex flex-col gap-4" aria-labelledby={`${id}-title`}>
      <div className="flex flex-col gap-1">
        <h2 id={`${id}-title`} className="text-2xl font-bold">
          {t("pharmacy:chifa.title")}
        </h2>
        <p className="text-ink-2">{t("pharmacy:chifa.intro")}</p>
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <Button variant="outline" disabled={openFile.isPending} onClick={choose}>
          <FileSpreadsheet aria-hidden />
          {t("pharmacy:chifa.choose")}
        </Button>
        <p className="flex flex-col">
          <span className="text-sm text-ink-2">{t("pharmacy:chifa.file")}</span>
          <span className="font-semibold">
            {file ? <bdi dir="auto">{file.fileName}</bdi> : t("pharmacy:chifa.noFile")}
          </span>
        </p>
      </div>
      {openFile.error ? (
        <p role="alert" className="text-danger">
          {errorText(openFile.error)}
        </p>
      ) : null}
      {applied ? (
        <p role="status" className="rounded-control bg-ok-soft p-3 font-semibold text-ok">
          {t("pharmacy:chifa.done", {
            created: formatNumber(applied.created, lang),
            updated: formatNumber(applied.updated, lang),
            matched: formatNumber(applied.matched, lang),
          })}
        </p>
      ) : null}
      {file ? (
        <>
          <fieldset className="flex flex-col gap-3">
            <legend className="mb-1 text-lg font-semibold">{t("pharmacy:chifa.mapping")}</legend>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
              {FIELDS.map((field) => {
                const value = mapping[field.key];
                return (
                  <Field key={field.key} id={`${id}-${field.key}`} label={t(field.label)}>
                    <NativeSelect
                      id={`${id}-${field.key}`}
                      value={value === null ? "" : String(value)}
                      onChange={(event) => setColumn(field.key, event.target.value)}
                    >
                      {field.key === "reimbursable" ? (
                        <option value="">{t("pharmacy:chifa.none")}</option>
                      ) : null}
                      {Array.from({ length: columnCount }, (_, i) => (
                        <option key={i} value={String(i)}>
                          {headers[i] ? `${i + 1}. ${headers[i]}` : String(i + 1)}
                        </option>
                      ))}
                    </NativeSelect>
                  </Field>
                );
              })}
            </div>
          </fieldset>
          {preview.error ? (
            <p role="alert" className="text-danger">
              {errorText(preview.error)}
            </p>
          ) : null}
          {shown.length > 0 ? (
            <div className="overflow-x-auto rounded-card border border-line bg-surface">
              <table className="w-full text-sm">
                <caption className="px-3 py-2 text-start font-semibold">
                  {t("pharmacy:chifa.preview")}
                </caption>
                <thead className="bg-surface-2 text-ink-2">
                  <tr>
                    {FIELDS.map((field) => (
                      <th key={field.key} scope="col" className="px-3 py-2 text-start font-semibold">
                        {t(field.label)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {shown.map((row, i) => (
                    <tr key={i} className="border-t border-line even:bg-surface-2">
                      {FIELDS.map((field) => (
                        <td key={field.key} className="px-3 py-2">
                          <bdi dir="auto">{cellOf(row, mapping[field.key])}</bdi>
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}
          {checked && upToDate ? (
            <div className="flex flex-col gap-2" role="status">
              <p className="font-semibold">
                {t("pharmacy:chifa.checked", {
                  rows: formatNumber(checked.result.rows, lang),
                  valid: formatNumber(checked.result.valid, lang),
                  matched: formatNumber(checked.result.matched, lang),
                })}
              </p>
              {checked.result.problems.length > 0 ? (
                <div className="flex flex-col gap-2 rounded-card border border-danger bg-danger-soft p-3">
                  <h3 className="font-semibold text-danger">{t("pharmacy:chifa.problems")}</h3>
                  <ul className="flex list-disc flex-col gap-1 ps-5 text-danger">
                    {checked.result.problems.slice(0, SHOWN_PROBLEMS).map((p) => (
                      <li key={`${p.row}:${p.column}`}>
                        {t("pharmacy:chifa.problem", {
                          row: p.row,
                          column: t(columnKey(p.column)),
                          reason: t(`pharmacy:chifa.reasons.${reasonKey(p)}`),
                        })}
                      </li>
                    ))}
                  </ul>
                  <p className="text-sm">{t("pharmacy:chifa.fixFirst")}</p>
                </div>
              ) : null}
            </div>
          ) : null}
          {check.error || apply.error ? (
            <p role="alert" className="text-danger">
              {errorText(check.error ?? apply.error)}
            </p>
          ) : null}
          <div className="flex flex-wrap justify-end gap-2">
            <Button variant="secondary" disabled={check.isPending} onClick={runCheck}>
              {t("pharmacy:chifa.check")}
            </Button>
            <Button variant="primary" disabled={!clean || apply.isPending} onClick={runApply}>
              {t("pharmacy:chifa.apply")}
            </Button>
          </div>
        </>
      ) : null}
    </section>
  );
}
