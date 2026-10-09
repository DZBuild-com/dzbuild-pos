import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { OrderFilter, OrderSaleRequest, PaymentInput, RefuseReason } from "./types";

export function useOnlineOrders(filter: OrderFilter) {
  const query = { filter, limit: 200 };
  return useQuery({ queryKey: qk.onlineOrders(query), queryFn: () => commands.cloudOrdersList(query) });
}

export function useOnlineOrder(id: string | null) {
  return useQuery({
    queryKey: qk.onlineOrder(id ?? ""),
    queryFn: () => commands.cloudOrdersGet(id ?? ""),
    enabled: id !== null,
  });
}

/** A step on an order: the list and the open order are read again (not the buyer's flag, which
 * may ask DZBuild). */
function useOrderMutation<A, R>(run: (args: A) => Promise<R>) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: run,
    onSettled: async () => {
      await client.invalidateQueries({ queryKey: ["onlineOrders", "list"] });
      await client.invalidateQueries({ queryKey: ["onlineOrders", "one"] });
    },
  });
}

export const useClaimOrder = () => useOrderMutation((id: string) => commands.cloudOrdersClaim(id));
export const useReleaseOrder = () => useOrderMutation((id: string) => commands.cloudOrdersRelease(id));
export const usePrepareOrder = () => useOrderMutation((id: string) => commands.cloudOrdersPrepare(id));
export const useRefuseOrder = () =>
  useOrderMutation(({ id, reason, note }: { id: string; reason: RefuseReason; note: string | null }) =>
    commands.cloudOrdersRefuse(id, reason, note),
  );
export const useSellOrder = () =>
  useOrderMutation((request: OrderSaleRequest) => commands.cloudOrdersSell(request));
/** An order paid online, handed over at the shop or to a courier (`courier`): never cash. */
export const useHandOverOrder = () =>
  useOrderMutation(
    ({
      id,
      label,
      courier,
      printReceipt,
    }: {
      id: string;
      label: string;
      courier: boolean;
      printReceipt: boolean;
    }) => commands.cloudOrdersHandOver(id, label, courier, printReceipt),
  );

/** The buyer paid DZBuild online (any method but cash on delivery): `orders::is_prepaid`. */
export function isPrepaid(paymentMethod: string): boolean {
  return paymentMethod.trim().toLowerCase() !== "cod";
}

export const useShipOrder = () =>
  useOrderMutation(({ id, label }: { id: string; label: string }) =>
    commands.cloudOrdersShip(id, label, null),
  );

/** Opened orders leave the unseen count (the rail's badge). */
export function useMarkSeen() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (ids: string[]) => commands.cloudOrdersSeen(ids),
    onSettled: async () => {
      await client.invalidateQueries({ queryKey: ["onlineOrders", "list"] });
      await client.invalidateQueries({ queryKey: qk.cloudStatus });
    },
  });
}

/** The order as the till prices it (online prices, delivery fee, duty, rounding) for `tenders`;
 * the figures stay on screen while the next tenders are priced. */
export function useOrderQuote(id: string, label: string, tenders: PaymentInput[]) {
  return useQuery({
    queryKey: qk.onlineOrderQuote(id, label, tenders),
    queryFn: () => commands.cloudOrdersQuote(id, label, tenders),
    placeholderData: keepPreviousData,
  });
}

/** DZBuild's flag on the buyer's phone (a warning, never a refusal). */
export function useCustomerFlag(phone: string | null) {
  return useQuery({
    queryKey: qk.customerFlag(phone),
    queryFn: () => commands.cloudCustomerFlag(phone ?? ""),
    enabled: Boolean(phone),
    staleTime: 60_000,
  });
}
