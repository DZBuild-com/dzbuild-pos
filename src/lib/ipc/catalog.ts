import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { CategoryInput } from "./types";

const LONG = 5 * 60_000;

export function useUnits() {
  return useQuery({ queryKey: qk.units, queryFn: () => commands.catalogUnitsList(), staleTime: LONG });
}

export function useCategories(includeArchived = false) {
  return useQuery({
    queryKey: qk.categories(includeArchived),
    queryFn: () => commands.catalogCategoriesList(includeArchived),
    staleTime: LONG,
  });
}

export function useTaxRates() {
  return useQuery({ queryKey: qk.taxRates, queryFn: () => commands.catalogTaxRatesList(), staleTime: LONG });
}

export function usePriceLevels() {
  return useQuery({
    queryKey: qk.priceLevels,
    queryFn: () => commands.catalogPriceLevelsList(),
    staleTime: LONG,
  });
}

function useRefreshCategories() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: ["catalog", "categories"] });
}

export function useCreateCategory() {
  const refresh = useRefreshCategories();
  return useMutation({
    mutationFn: (input: CategoryInput) => commands.catalogCategoriesCreate(input),
    onSuccess: refresh,
  });
}

export function useUpdateCategory() {
  const refresh = useRefreshCategories();
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: CategoryInput }) =>
      commands.catalogCategoriesUpdate(id, input),
    onSuccess: refresh,
  });
}

export function useArchiveCategory() {
  const refresh = useRefreshCategories();
  return useMutation({
    mutationFn: (id: string) => commands.catalogCategoriesArchive(id),
    onSuccess: refresh,
  });
}
