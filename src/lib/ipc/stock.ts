import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { AdjustInput } from "./types";

export function useStockLocations() {
  return useQuery({
    queryKey: qk.stockLocations,
    queryFn: () => commands.stockLocations(),
    staleTime: 5 * 60_000,
  });
}

export function useStockLevels(productId: string | null) {
  return useQuery({
    queryKey: qk.stockLevels(productId ?? ""),
    queryFn: () => commands.stockLevels(productId ?? ""),
    enabled: productId !== null,
  });
}

export function useStockMovements(productId: string | null, limit = 50) {
  return useQuery({
    queryKey: qk.stockMovements(productId ?? ""),
    queryFn: () => commands.stockMovements(productId ?? "", limit),
    enabled: productId !== null,
  });
}

export function useLowStock(limit = 200) {
  return useQuery({ queryKey: qk.stockLow, queryFn: () => commands.stockLow(limit) });
}

export function useAdjustStock() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (input: AdjustInput) => commands.stockAdjust(input),
    onSuccess: () =>
      Promise.all([
        client.invalidateQueries({ queryKey: ["stock"] }),
        client.invalidateQueries({ queryKey: ["products"] }),
      ]),
  });
}
