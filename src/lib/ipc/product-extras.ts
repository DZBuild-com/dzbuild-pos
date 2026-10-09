import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type {
  BulkEdit,
  BulkPriceInput,
  PriceBreakInput,
  PriceHelpInput,
  ProductExtrasInput,
  VariantsInput,
} from "./types";

/** Every product list, card and search shows what these change. */
function useRefreshProducts() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: ["products"] });
}

export function useProductExtras(id: string | null) {
  return useQuery({
    queryKey: qk.productExtras(id ?? ""),
    queryFn: () => commands.productsExtrasGet(id ?? ""),
    enabled: id !== null,
  });
}

/** The extras of several products at once, in the order of `ids` (whether purchase lines take a
 * lot or serial numbers). */
export function useExtrasOf(ids: readonly string[]) {
  return useQueries({
    queries: ids.map((id) => ({
      queryKey: qk.productExtras(id),
      queryFn: () => commands.productsExtrasGet(id),
      staleTime: 60_000,
    })),
  });
}

export function useSetProductExtras() {
  const refresh = useRefreshProducts();
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: ProductExtrasInput }) =>
      commands.productsExtrasSet(id, input),
    onSuccess: refresh,
  });
}

export function usePriceBreaks(id: string | null) {
  return useQuery({
    queryKey: qk.priceBreaks(id ?? ""),
    queryFn: () => commands.productsBreaksList(id ?? ""),
    enabled: id !== null,
  });
}

export function useReplacePriceBreaks() {
  const refresh = useRefreshProducts();
  return useMutation({
    mutationFn: ({ id, breaks }: { id: string; breaks: PriceBreakInput[] }) =>
      commands.productsBreaksReplace(id, breaks),
    onSuccess: refresh,
  });
}

/** Takes the next in-store EAN-13 (the counter moves even if the product is not saved). */
export function useNextEan13() {
  return useMutation({ mutationFn: () => commands.productsNextEan13() });
}

export function useNextReference() {
  return useMutation({ mutationFn: () => commands.productsNextReference() });
}

export function useGenerateVariants() {
  const refresh = useRefreshProducts();
  return useMutation({
    mutationFn: ({ parentId, input }: { parentId: string; input: VariantsInput }) =>
      commands.productsVariantsGenerate(parentId, input),
    onSuccess: refresh,
  });
}

/** The prices before and after, from the backend (the UI computes no price). */
export function useBulkPricePreview(input: BulkPriceInput | null) {
  return useQuery({
    queryKey: qk.bulkPricePreview(input),
    queryFn: () => commands.productsBulkPricePreview(input as BulkPriceInput),
    enabled: input !== null,
  });
}

export function useApplyBulkPrice() {
  const refresh = useRefreshProducts();
  return useMutation({
    mutationFn: (input: BulkPriceInput) => commands.productsBulkPriceApply(input),
    onSuccess: refresh,
  });
}

export function useBulkEdit() {
  const refresh = useRefreshProducts();
  return useMutation({
    mutationFn: (edits: BulkEdit[]) => commands.productsBulkEdit(edits),
    onSuccess: refresh,
  });
}

/** Margin, markup and the price for a target margin, from the backend. */
export function usePriceHelp(input: PriceHelpInput | null) {
  return useQuery({
    queryKey: qk.priceHelp(input),
    queryFn: () => commands.productsPriceHelp(input as PriceHelpInput),
    enabled: input !== null,
  });
}

/** Opens the Rust file dialog; resolves to `null` when the user cancels it. */
export function usePickImage() {
  const refresh = useRefreshProducts();
  return useMutation({ mutationFn: (id: string) => commands.productsPickImage(id), onSuccess: refresh });
}

export function useClearImage() {
  const refresh = useRefreshProducts();
  return useMutation({ mutationFn: (id: string) => commands.productsClearImage(id), onSuccess: refresh });
}

/** A folder of photos named by barcode or reference (IMX-018); `null` when cancelled. */
export function useImportImages() {
  const refresh = useRefreshProducts();
  return useMutation({ mutationFn: () => commands.productsImportImages(), onSuccess: refresh });
}
