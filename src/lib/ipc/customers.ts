import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { CustomerInput, CustomerQuery } from "./types";

/** A directory search with the filters left out at their defaults (every active customer). */
export function customerQuery(filter: Partial<CustomerQuery>): CustomerQuery {
  return {
    text: "",
    customerType: null,
    withDebt: false,
    blocked: null,
    includeArchived: false,
    limit: null,
    ...filter,
  };
}

/** The directory (back office) and the F3 search at the till; balances come back as 0 without
 * `credit.view`. */
export function useCustomerSearch(filter: Partial<CustomerQuery>, enabled = true) {
  const query = customerQuery(filter);
  return useQuery({
    queryKey: qk.customerSearch(query),
    queryFn: () => commands.customersSearch(query),
    enabled,
    placeholderData: keepPreviousData,
    staleTime: 5_000,
  });
}

export function useCustomer(id: string | null) {
  return useQuery({
    queryKey: qk.customer(id ?? ""),
    queryFn: () => commands.customersGet(id ?? ""),
    enabled: id !== null,
  });
}

/** The customer's file, from the cache while it is fresh: for a checkout that opens no dialog and
 * still prints in the customer's language. */
export function useFetchCustomer() {
  const client = useQueryClient();
  return (id: string) =>
    client.fetchQuery({
      queryKey: qk.customer(id),
      queryFn: () => commands.customersGet(id),
      staleTime: 30_000,
    });
}

/** A typed or read phone number is looked up every time (CRM-001). */
export function useFindCustomerByPhone() {
  return useMutation({ mutationFn: (phone: string) => commands.customersFindByPhone(phone) });
}

export function useCustomerDocuments(id: string | null, limit = 50) {
  return useQuery({
    queryKey: qk.customerDocuments(id ?? "", limit),
    queryFn: () => commands.customersDocuments(id ?? "", limit),
    enabled: id !== null,
  });
}

/** SAL-029: the customer's last ticket with its lines, or `null` when he has none (his return
 * notes and invoices are skipped). */
export function useLastTicket() {
  return useMutation({
    mutationFn: async (customerId: string) => {
      const recent = await commands.customersDocuments(customerId, 20);
      const last = recent.find((d) => d.docType === "ticket");
      return last ? commands.documentsGet(last.id) : null;
    },
  });
}

function useRefreshCustomers() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: qk.customers });
}

export function useCreateCustomer() {
  const refresh = useRefreshCustomers();
  return useMutation({
    mutationFn: (input: CustomerInput) => commands.customersCreate(input),
    onSuccess: refresh,
  });
}

/** CRM-003: a name and maybe a phone, from the till. */
export function useQuickCreateCustomer() {
  const refresh = useRefreshCustomers();
  return useMutation({
    mutationFn: ({ name, phone }: { name: string; phone: string | null }) =>
      commands.customersQuickCreate(name, phone),
    onSuccess: refresh,
  });
}

export function useUpdateCustomer() {
  const refresh = useRefreshCustomers();
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: CustomerInput }) => commands.customersUpdate(id, input),
    onSuccess: refresh,
  });
}

export function useSetCustomerBlocked() {
  const refresh = useRefreshCustomers();
  return useMutation({
    mutationFn: ({ id, blocked }: { id: string; blocked: boolean }) =>
      commands.customersSetBlocked(id, blocked),
    onSuccess: refresh,
  });
}

export function useArchiveCustomer() {
  const refresh = useRefreshCustomers();
  return useMutation({ mutationFn: (id: string) => commands.customersArchive(id), onSuccess: refresh });
}

export function useRestoreCustomer() {
  const refresh = useRefreshCustomers();
  return useMutation({ mutationFn: (id: string) => commands.customersRestore(id), onSuccess: refresh });
}
