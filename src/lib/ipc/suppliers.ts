import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import { type EntryPages, useEntryPages } from "./credit";
import type { Money, SupplierInput, SupplierPaymentInput, SupplierQuery } from "./types";

/** A supplier search with the filters left out at their defaults (every active supplier). */
export function supplierQuery(filter: Partial<SupplierQuery>): SupplierQuery {
  return { text: "", withBalance: false, includeArchived: false, limit: null, ...filter };
}

export function useSupplierSearch(filter: Partial<SupplierQuery>, enabled = true) {
  const query = supplierQuery(filter);
  return useQuery({
    queryKey: qk.supplierSearch(query),
    queryFn: () => commands.suppliersSearch(query),
    enabled,
    placeholderData: keepPreviousData,
    staleTime: 5_000,
  });
}

export function useSupplier(id: string | null) {
  return useQuery({
    queryKey: qk.supplier(id ?? ""),
    queryFn: () => commands.suppliersGet(id ?? ""),
    enabled: id !== null,
  });
}

function useRefreshSuppliers() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: qk.suppliers });
}

/** A payment moves the account, maybe the drawer (`fromTill`), and the reports. */
function useRefreshAfterPayment() {
  const client = useQueryClient();
  return () =>
    Promise.all(
      [qk.suppliers, qk.purchases, ["cash"], ["reports"]].map((queryKey) =>
        client.invalidateQueries({ queryKey }),
      ),
    );
}

export function useCreateSupplier() {
  const refresh = useRefreshSuppliers();
  return useMutation({
    mutationFn: (input: SupplierInput) => commands.suppliersCreate(input),
    onSuccess: refresh,
  });
}

export function useUpdateSupplier() {
  const refresh = useRefreshSuppliers();
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: SupplierInput }) => commands.suppliersUpdate(id, input),
    onSuccess: refresh,
  });
}

export function useArchiveSupplier() {
  const refresh = useRefreshSuppliers();
  return useMutation({ mutationFn: (id: string) => commands.suppliersArchive(id), onSuccess: refresh });
}

export function useRestoreSupplier() {
  const refresh = useRefreshSuppliers();
  return useMutation({ mutationFn: (id: string) => commands.suppliersRestore(id), onSuccess: refresh });
}

export function useSupplierOpening() {
  const refresh = useRefreshSuppliers();
  return useMutation({
    mutationFn: ({ supplierId, amount, note }: { supplierId: string; amount: Money; note: string | null }) =>
      commands.suppliersSetOpening(supplierId, amount, note),
    onSuccess: refresh,
  });
}

export function useSupplierAdjust() {
  const refresh = useRefreshSuppliers();
  return useMutation({
    mutationFn: ({ supplierId, amount, note }: { supplierId: string; amount: Money; note: string }) =>
      commands.suppliersAdjust(supplierId, amount, note),
    onSuccess: refresh,
  });
}

export function usePaySupplier() {
  const refresh = useRefreshAfterPayment();
  return useMutation({
    mutationFn: (input: SupplierPaymentInput) => commands.suppliersPay(input),
    onSuccess: refresh,
  });
}

/** A supplier's account, newest first, older pages on demand. */
export function useSupplierEntryPages(supplierId: string | null): EntryPages {
  return useEntryPages(
    qk.supplier(supplierId ?? ""),
    (query) => commands.suppliersEntries(supplierId ?? "", query),
    supplierId !== null,
  );
}

/** The supplier invoices a payment can settle, oldest first. */
export function useSupplierOpenItems(supplierId: string | null) {
  return useQuery({
    queryKey: [...qk.supplier(supplierId ?? ""), "open"],
    queryFn: () => commands.suppliersOpenItems(supplierId ?? ""),
    enabled: supplierId !== null,
  });
}

export function useSupplierStatement(supplierId: string | null, from: string, to: string) {
  return useQuery({
    queryKey: [...qk.supplier(supplierId ?? ""), "statement", from, to],
    queryFn: () => commands.suppliersStatement(supplierId ?? "", from, to),
    enabled: supplierId !== null,
  });
}

/** The ageing of what the shop owes (`purchases.view`: pass `enabled` false without it). */
export function useSupplierAging(asOf: string, enabled = true) {
  return useQuery({
    queryKey: qk.supplierAging(asOf),
    queryFn: () => commands.suppliersAging(asOf),
    enabled,
  });
}

export function usePayables() {
  return useQuery({ queryKey: qk.payables, queryFn: () => commands.suppliersPayables() });
}
