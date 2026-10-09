import { keepPreviousData, useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { RunInput, WasteInput, YieldTemplateInput } from "./types";

/** A yield run or a waste record moves stock: the stock screens refresh with the butcher's. */
function useRefreshButcher() {
  const client = useQueryClient();
  return () =>
    Promise.all([
      client.invalidateQueries({ queryKey: qk.butcher.all }),
      client.invalidateQueries({ queryKey: ["stock"] }),
    ]);
}

/** The carcass and cuts of the cutting templates (the products, in the order of `ids`), named in
 * the UI language by the screen. Here, not in `./products`: the till's entry never loads it. */
export function useProductsOf(ids: readonly string[]) {
  return useQueries({
    queries: ids.map((id) => ({ queryKey: qk.product(id), queryFn: () => commands.productsGet(id) })),
  });
}

export function useYieldTemplates(enabled = true) {
  return useQuery({ queryKey: qk.butcher.templates, queryFn: () => commands.yieldTemplates(), enabled });
}

export function useSaveYieldTemplate() {
  const refresh = useRefreshButcher();
  return useMutation({
    mutationFn: ({ id, input }: { id: string | null; input: YieldTemplateInput }) =>
      commands.yieldTemplateSave(id, input),
    onSuccess: refresh,
  });
}

export function useArchiveYieldTemplate() {
  const refresh = useRefreshButcher();
  return useMutation({ mutationFn: (id: string) => commands.yieldTemplateArchive(id), onSuccess: refresh });
}

/** A run is worth previewing once it names its carcass, a weight and at least one cut. */
function previewable(input: RunInput | null): input is RunInput {
  return input !== null && input.sourceProductId !== "" && input.inputQty > 0 && input.cuts.length > 0;
}

/** The run's costs, yields and loss computed by the backend (nothing written); `null` waits. */
export function useYieldPreview(input: RunInput | null) {
  return useQuery({
    queryKey: qk.butcher.preview(input),
    queryFn: () => commands.yieldPreview(input as RunInput),
    enabled: previewable(input),
    placeholderData: keepPreviousData,
  });
}

export function useYieldRun() {
  const refresh = useRefreshButcher();
  return useMutation({ mutationFn: (input: RunInput) => commands.yieldRun(input), onSuccess: refresh });
}

export function useYieldRuns(from: string, to: string) {
  return useQuery({ queryKey: qk.butcher.runs(from, to), queryFn: () => commands.yieldRuns(from, to) });
}

export function useRecordWaste() {
  const refresh = useRefreshButcher();
  return useMutation({ mutationFn: (input: WasteInput) => commands.wasteRecord(input), onSuccess: refresh });
}

/** The waste of one business day (`YYYY-MM-DD`) per product and reason. */
export function useWasteDay(day: string) {
  return useQuery({ queryKey: qk.butcher.waste(day), queryFn: () => commands.wasteDay(day) });
}
