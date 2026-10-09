import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { ExchangeRequest, ReturnExtras, ReturnInput, ReturnRequest } from "./types";

export function useReturnable(originalId: string | null) {
  return useQuery({
    queryKey: qk.returnable(originalId ?? ""),
    queryFn: () => commands.returnsReturnable(originalId ?? ""),
    enabled: originalId !== null,
    staleTime: 0,
  });
}

/** The exact refund (a dry run on the backend): nothing is kept. `extras` are the return's pack
 * extras (the serials coming back, the exchange voucher), quoted as the return will make them. */
export function useReturnQuote(extras: ReturnExtras | null = null) {
  return useMutation({ mutationFn: (input: ReturnInput) => commands.returnsQuote(input, extras) });
}

/** A return refunded to the account (RET-007) moves the customer's balance too. */
function useRefreshAfterReturn() {
  const client = useQueryClient();
  return () =>
    Promise.all(
      [["cash"], ["documents"], ["stock"], ["returns"], ["customers"], ["credit"]].map((queryKey) =>
        client.invalidateQueries({ queryKey }),
      ),
    );
}

export function useCreateReturn() {
  const refresh = useRefreshAfterReturn();
  return useMutation({
    mutationFn: (request: ReturnRequest) => commands.returnsCreate(request),
    onSuccess: refresh,
  });
}

export function useExchange() {
  const refresh = useRefreshAfterReturn();
  return useMutation({
    mutationFn: (request: ExchangeRequest) => commands.returnsExchange(request),
    onSuccess: refresh,
  });
}
