import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Lang } from "@/lib/i18n";
import { useDebounced } from "@/lib/use-debounced";
import { commands } from "./client";
import { qk } from "./keys";
import type {
  LabelPreviewInput,
  LabelPrintInput,
  LabelRequest,
  LabelTemplate,
  ScaleItemInput,
  SheetFormat,
  SheetInput,
  WeighedLabelInput,
} from "./types";

export function useLabelTemplates() {
  return useQuery({ queryKey: qk.labels, queryFn: () => commands.labelsTemplates(), staleTime: 60_000 });
}

export function useSaveLabelTemplates() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({
      templates,
      categoryTemplates,
    }: {
      templates: LabelTemplate[];
      categoryTemplates: Record<string, string>;
    }) => commands.labelsSaveTemplates(templates, categoryTemplates),
    onSuccess: (saved) => {
      client.setQueryData(qk.labels, saved);
      // what was built from the old templates: the batches, their planches and the settings
      void client.invalidateQueries({ queryKey: ["labels", "build"] });
      void client.invalidateQueries({ queryKey: ["labels", "sheet"] });
      void client.invalidateQueries({ queryKey: qk.setting("labels.templates") });
      void client.invalidateQueries({ queryKey: qk.setting("labels.category_templates") });
    },
  });
}

/** The labels of `request` with their warnings (`null`: nothing asked yet). */
export function useBuildLabels(request: LabelRequest | null) {
  return useQuery({
    queryKey: qk.labelBatch(request),
    queryFn: () => commands.labelsBuild(request as LabelRequest),
    enabled: request !== null,
  });
}

/** The designer's preview: rebuilt 300 ms after the last change. The input is debounced as its
 * JSON text, so a new object with the same contents on every render asks nothing again. */
export function useLabelPreview(input: LabelPreviewInput | null) {
  const key = useDebounced(input === null ? null : JSON.stringify(input), 300);
  return useQuery({
    queryKey: qk.labelPreview(key),
    queryFn: () => commands.labelsPreview(JSON.parse(key as string) as LabelPreviewInput),
    enabled: key !== null,
    placeholderData: keepPreviousData,
  });
}

export function usePrintLabels() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (input: LabelPrintInput) => commands.labelsPrint(input),
    onSuccess: (done) => {
      void client.invalidateQueries({ queryKey: qk.staleLabels });
      // internal EAN-13s were given: the products and the batch's warnings changed
      if (done.assigned.length > 0) {
        void client.invalidateQueries({ queryKey: ["products"] });
        void client.invalidateQueries({ queryKey: ["labels", "build"] });
      }
    },
  });
}

/** The A4 planche of `input` (`null`: nothing asked yet), kept on screen while the next one loads. */
export function useLabelSheet(input: SheetInput | null) {
  return useQuery({
    queryKey: qk.labelSheet(input),
    queryFn: () => commands.labelsSheet(input as SheetInput),
    enabled: input !== null,
    placeholderData: keepPreviousData,
  });
}

/** Prints the planche on this PC's A4 printer; Rust journals the labels once the print host has
 * printed it, so they leave the reprint queue. */
export function useSheetPrint() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (input: SheetInput) => commands.labelsSheetPrint(input),
    onSuccess: () => void client.invalidateQueries({ queryKey: qk.staleLabels }),
  });
}

/** Saves the planche as a PDF where the Rust save dialog says: `{ path }`, or `null` when the user
 * cancels it (nothing is journaled then). */
export function useSheetPdf() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (input: SheetInput) => commands.labelsSheetExportPdf(input),
    onSuccess: (saved) => {
      if (saved) void client.invalidateQueries({ queryKey: qk.staleLabels });
    },
  });
}

export function usePrintWeighedLabel() {
  return useMutation({ mutationFn: (input: WeighedLabelInput) => commands.labelsPrintWeighed(input) });
}

export function useStaleLabels() {
  return useQuery({ queryKey: qk.staleLabels, queryFn: () => commands.labelsStale(null) });
}

export function useDismissLabels() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (productIds: string[]) => commands.labelsDismiss(productIds),
    onSuccess: () => void client.invalidateQueries({ queryKey: qk.staleLabels }),
  });
}

/** The goods of the label scales (`enabled: false` until a screen needs them). */
export function useScaleItems(enabled = true) {
  return useQuery({ queryKey: qk.scaleItems, queryFn: () => commands.scaleItemsList(), enabled });
}

export function useSetScaleItem() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ productId, input }: { productId: string; input: ScaleItemInput }) =>
      commands.scaleItemsSet(productId, input),
    onSuccess: () => void client.invalidateQueries({ queryKey: qk.scaleItems }),
  });
}

/** The PLU list after a Rust save dialog: the file name, `null` when cancelled. */
export function useExportPlu() {
  return useMutation({
    mutationFn: ({ format, lang }: { format: SheetFormat; lang: Lang }) =>
      commands.scalePluExport(format, lang),
  });
}

/** A data collector's file into the open count `countId`: `null` when the dialog is cancelled. */
export function useImportCollector() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (countId: string) => commands.countsImportCollector(countId),
    onSuccess: () => void client.invalidateQueries({ queryKey: qk.counts }),
  });
}
