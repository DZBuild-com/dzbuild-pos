import { useMutation, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";

// The two restaurant writes the till's print failures call (Réimprimer on a kitchen ticket or an
// addition), apart from `restaurant.ts` (whose copies the pack screens use): the /pos chunk
// carries no restaurant hook and shares no chunk with the pack screens.

function useRefreshDining() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: qk.dining.all });
}

/** The addition (printed by the backend); `parts` asks for even shares. */
export function usePrebill() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ ticketId, parts = null }: { ticketId: string; parts?: number | null }) =>
      commands.diningTicketPrebill(ticketId, parts),
    onSuccess: refresh,
  });
}

/** Prints a kitchen ticket again (one whose print failed). */
export function useKitchenReprint() {
  return useMutation({ mutationFn: (id: string) => commands.kitchenReprint(id) });
}
