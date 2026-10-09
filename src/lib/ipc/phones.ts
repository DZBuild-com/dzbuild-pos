import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { FloatPurchase, Money } from "./types";

/** The serials still in stock of a product: what the till's serial prompt offers. */
export function useSerialsAvailable(productId: string | null) {
  return useQuery({
    queryKey: qk.serialsAvailable(productId ?? ""),
    queryFn: () => commands.serialsAvailable(productId ?? ""),
    enabled: productId !== null,
  });
}

/** The data pack's mobile operators with the shop's commission and the float left of each. */
export function useFlexyOperators() {
  return useQuery({ queryKey: qk.flexyOperators, queryFn: () => commands.flexyOperators() });
}

/** The cart line of a top-up (the number in its note); the till adds it like any line. */
export function useFlexyLine() {
  return useMutation({
    mutationFn: ({ operator, phone, amount }: { operator: string; phone: string; amount: Money }) =>
      commands.flexyLine(operator, phone, amount),
  });
}

/** Float bought from the distributor (on a till: on the main PC); the drawer pays it when asked. */
export function useFlexyPurchase() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (input: FloatPurchase) => commands.flexyPurchase(input),
    onSuccess: () =>
      Promise.all(
        [qk.flexy, ["cash"], ["reports"]].map((queryKey) => client.invalidateQueries({ queryKey })),
      ),
  });
}

/** The float left per operator, `[operator, balance]`. */
export function useFlexyBalances() {
  return useQuery({ queryKey: qk.flexyBalances, queryFn: () => commands.flexyBalances() });
}

/** The float ledger of the business days `from`..=`to`, of one operator or all (`reports.cash`). */
export function useFlexyEntries(operator: string | null, from: string, to: string) {
  return useQuery({
    queryKey: qk.flexyEntries(operator, from, to),
    queryFn: () => commands.flexyEntries(operator, from, to),
  });
}
