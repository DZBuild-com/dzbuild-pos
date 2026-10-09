import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { PromotionInput } from "./types";

/** A promotion changes the lists and the prices of the carts being rung up. */
function useRefreshPromotions() {
  const client = useQueryClient();
  return () =>
    Promise.all([qk.promotions, ["sales"]].map((queryKey) => client.invalidateQueries({ queryKey })));
}

export function usePromotions(includeArchived = false) {
  return useQuery({
    queryKey: qk.promotionList(includeArchived),
    queryFn: () => commands.promotionsList(includeArchived),
  });
}

export function usePromotion(id: string | null) {
  return useQuery({
    queryKey: qk.promotion(id ?? ""),
    queryFn: () => commands.promotionsGet(id ?? ""),
    enabled: id !== null,
  });
}

/** The named discounts running today (PRM-003), for the till. */
export function usePresets(enabled = true) {
  return useQuery({ queryKey: qk.presets, queryFn: () => commands.promotionsPresets(), enabled });
}

/** Creates the promotion, or updates it when `id` is given. */
export function useSavePromotion() {
  const refresh = useRefreshPromotions();
  return useMutation({
    mutationFn: ({ id, input }: { id?: string | null; input: PromotionInput }) =>
      id ? commands.promotionsUpdate(id, input) : commands.promotionsCreate(input),
    onSuccess: refresh,
  });
}

export function useArchivePromotion() {
  const refresh = useRefreshPromotions();
  return useMutation({ mutationFn: (id: string) => commands.promotionsArchive(id), onSuccess: refresh });
}
