import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { InvoiceInput, OrderInput, PurchaseQuery, ReceiptInput, SupplierReturnInput } from "./types";

/** Supplier documents with the filters left out at their defaults (all of them, newest first). */
export function purchaseQuery(filter: Partial<PurchaseQuery>): PurchaseQuery {
  return { docType: null, supplierId: null, from: null, to: null, limit: null, ...filter };
}

export function usePurchases(filter: Partial<PurchaseQuery>) {
  const query = purchaseQuery(filter);
  return useQuery({
    queryKey: qk.purchaseList(query),
    queryFn: () => commands.purchasesList(query),
    placeholderData: keepPreviousData,
  });
}

export function usePurchase(id: string | null) {
  return useQuery({
    queryKey: qk.purchase(id ?? ""),
    queryFn: () => commands.purchasesGet(id ?? ""),
    enabled: id !== null,
  });
}

/** What each line of a purchase order still waits for. */
export function useRemaining(orderId: string | null) {
  return useQuery({
    queryKey: [...qk.purchase(orderId ?? ""), "remaining"],
    queryFn: () => commands.purchasesRemaining(orderId ?? ""),
    enabled: orderId !== null,
  });
}

/** The CMUP changes of a product (zeros without `reports.profit`). */
export function useCostHistory(productId: string | null) {
  return useQuery({
    queryKey: qk.costHistory(productId ?? ""),
    queryFn: () => commands.purchasesCostHistory(productId ?? ""),
    enabled: productId !== null,
  });
}

function useRefresh(keys: readonly (readonly unknown[])[]) {
  const client = useQueryClient();
  return () => Promise.all(keys.map((queryKey) => client.invalidateQueries({ queryKey })));
}

/** Orders move only the purchases list. */
const ORDER_KEYS = [qk.purchases] as const;
/** Receipts and returns move the stock and the average cost, invoices and payments the supplier
 * account and maybe the drawer. */
const STOCK_KEYS = [qk.purchases, qk.suppliers, ["stock"], ["products"], ["cash"], ["reports"]] as const;
const ACCOUNT_KEYS = [qk.purchases, qk.suppliers, ["cash"], ["reports"]] as const;

export function useSaveOrder() {
  const refresh = useRefresh(ORDER_KEYS);
  return useMutation({
    mutationFn: ({ id, input }: { id: string | null; input: OrderInput }) =>
      commands.purchasesSaveOrder(id, input),
    onSuccess: refresh,
  });
}

export function useValidateOrder() {
  const refresh = useRefresh(ORDER_KEYS);
  return useMutation({ mutationFn: (id: string) => commands.purchasesValidateOrder(id), onSuccess: refresh });
}

export function useCancelOrder() {
  const refresh = useRefresh(ORDER_KEYS);
  return useMutation({ mutationFn: (id: string) => commands.purchasesCancelOrder(id), onSuccess: refresh });
}

export function useReceive() {
  const refresh = useRefresh(STOCK_KEYS);
  return useMutation({
    mutationFn: (input: ReceiptInput) => commands.purchasesReceive(input),
    onSuccess: refresh,
  });
}

export function useRecordInvoice() {
  const refresh = useRefresh(ACCOUNT_KEYS);
  return useMutation({
    mutationFn: (input: InvoiceInput) => commands.purchasesRecordInvoice(input),
    onSuccess: refresh,
  });
}

export function useSupplierReturn() {
  const refresh = useRefresh(STOCK_KEYS);
  return useMutation({
    mutationFn: (input: SupplierReturnInput) => commands.purchasesReturn(input),
    onSuccess: refresh,
  });
}
