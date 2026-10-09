import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import type { Lang } from "@/lib/i18n";
import { commands, events } from "./client";
import { qk } from "./keys";
import type { ImportEntity, ImportOptions, ImportProgress, Problem } from "./types";

/** What an import of each kind changes, besides the import history and the reports. */
const TOUCHED: Record<ImportEntity, ReadonlyArray<readonly unknown[]>> = {
  products: [["products"], ["catalog"], ["stock"]],
  categories: [["catalog"], ["products"]],
  customers: [qk.customers],
  suppliers: [qk.suppliers],
  opening_stock: [["stock"], ["products"]],
  customer_balances: [qk.customers, ["credit"]],
  supplier_balances: [qk.suppliers],
  count_lines: [qk.counts],
};

/** After an import or its undo: the records of that kind, the history, the reports. */
function useRefreshAfterImport() {
  const client = useQueryClient();
  return (kind: ImportEntity) =>
    Promise.all(
      [...TOUCHED[kind], qk.imports, qk.reports, qk.dashboard].map((queryKey) =>
        client.invalidateQueries({ queryKey }),
      ),
    );
}

/** Opens the Rust file dialog and reads the file; resolves `null` when the user cancels it. */
export function useOpenImportFile() {
  return useMutation({ mutationFn: () => commands.importsOpenFile() });
}

/** The columns, the first rows and the suggested mapping of a sheet of the opened file, under the
 * title row the reader found (`headerRow: null`) or the one the user chose. */
export function useImportPreview(
  token: string | null,
  sheet: number,
  entity: ImportEntity,
  headerRow: number | null = null,
) {
  return useQuery({
    queryKey: qk.importPreview(token, sheet, entity, headerRow),
    queryFn: () => commands.importsPreview(token ?? "", sheet, entity, headerRow),
    enabled: token !== null,
    // Another title row of the same sheet keeps the columns on screen while the new ones are read.
    placeholderData: (previous, previousQuery) =>
      previousQuery?.queryKey[2] === token && previousQuery.queryKey[3] === sheet ? previous : undefined,
    // The opened file does not change; after the import it is gone from the backend.
    staleTime: Infinity,
  });
}

type ImportRun = { token: string; sheet: number; options: ImportOptions };

/** Checks the file with these options (nothing is kept); only they may then be applied. */
export function useDryRun() {
  return useMutation({
    mutationFn: ({ token, sheet, options }: ImportRun) => commands.importsDryRun(token, sheet, options),
  });
}

/** Applies the options the last check used (`import_mapping_invalid` for any other). */
export function useApplyImport() {
  const refresh = useRefreshAfterImport();
  return useMutation({
    mutationFn: ({ token, sheet, options }: ImportRun) => commands.importsApply(token, sheet, options),
    onSuccess: (job) => refresh(job.kind),
  });
}

/** The batches of the running import (`import-progress`): `onProgress` receives each one. */
export function useImportProgress(onProgress: (progress: ImportProgress) => void): void {
  const handler = useRef(onProgress);
  useEffect(() => {
    handler.current = onProgress;
  }, [onProgress]);
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    void events.importProgress
      .listen((event) => handler.current(event.payload))
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      // Without the progress the import still runs and its result arrives: log the refusal.
      .catch((error: unknown) => console.error("import-progress listener refused:", error));
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);
}

/** Saves the problems as an xlsx file where the user says; resolves the path, or `null`. */
export function useSaveErrorReport() {
  return useMutation({
    mutationFn: ({ entity, problems, lang }: { entity: ImportEntity; problems: Problem[]; lang: Lang }) =>
      commands.importsErrorReport(entity, problems, lang),
  });
}

/** Saves the template of an import in `lang` where the user says; resolves the path, or `null`. */
export function useSaveTemplate() {
  return useMutation({
    mutationFn: ({ entity, lang }: { entity: ImportEntity; lang: Lang }) =>
      commands.importsTemplate(entity, lang),
  });
}

/** The latest imports of the kinds the user may import, newest first. */
export function useImportJobs(enabled = true) {
  return useQuery({ queryKey: qk.importJobs, queryFn: () => commands.importsJobs(), enabled });
}

/** Undoes the latest import of the day. */
export function useUndoImport() {
  const refresh = useRefreshAfterImport();
  return useMutation({
    mutationFn: (jobId: string) => commands.importsUndo(jobId),
    onSuccess: (job) => refresh(job.kind),
  });
}

/** Drops the opened file when the wizard closes without importing. */
export function useDiscardImport() {
  return useMutation({ mutationFn: (token: string) => commands.importsDiscard(token) });
}
