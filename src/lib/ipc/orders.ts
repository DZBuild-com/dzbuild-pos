import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type {
  AppointmentInput,
  AppointmentStatus,
  CustomerOrderFilter,
  CustomerOrderInput,
  DeliverInput,
  FulfilInput,
  OrderDeposit,
} from "./types";

/** Invalidates every query under these keys once a write succeeded. */
function useRefresh(keys: ReadonlyArray<readonly unknown[]>) {
  const client = useQueryClient();
  return () => Promise.all(keys.map((queryKey) => client.invalidateQueries({ queryKey })));
}

/** After money moved on an order: the orders, deposits, customers, drawer, stock and reports. */
const ORDER_MONEY_KEYS: ReadonlyArray<readonly unknown[]> = [
  qk.orders,
  qk.deposits,
  qk.customers,
  ["credit"],
  ["cash"],
  ["stock"],
  ["reports"],
];

/** Records an order (its deposit's `AC…` receipt prints when it takes one). */
export function useCreateOrder() {
  const refresh = useRefresh([...ORDER_MONEY_KEYS, ["documents"], ["archive"]]);
  return useMutation({
    mutationFn: (input: CustomerOrderInput) => commands.ordersCreate(input),
    onSuccess: refresh,
  });
}

export function useOrder(id: string | null) {
  return useQuery({
    queryKey: qk.order(id ?? ""),
    queryFn: () => commands.ordersGet(id ?? ""),
    enabled: id !== null,
  });
}

export function useOrders(filter: CustomerOrderFilter) {
  return useQuery({
    queryKey: qk.orderList(filter),
    queryFn: () => commands.ordersList(filter),
    placeholderData: keepPreviousData,
  });
}

/** The order's lines as a till cart at the ordered prices (read when the till serves it, afresh
 * each time); `null` asks nothing. */
export function useFulfilCart(id: string | null) {
  return useQuery({
    queryKey: [...qk.order(id ?? ""), "cart"],
    queryFn: () => commands.ordersFulfilCart(id ?? ""),
    enabled: id !== null,
    staleTime: 0,
    gcTime: 0,
  });
}

/** A further deposit on an order (its `AC…` receipt prints). */
export function usePayIn() {
  const refresh = useRefresh(ORDER_MONEY_KEYS);
  return useMutation({
    mutationFn: ({ id, deposit }: { id: string; deposit: OrderDeposit }) => commands.ordersPayIn(id, deposit),
    onSuccess: refresh,
  });
}

export function useSetOrderReady() {
  const refresh = useRefresh([qk.orders]);
  return useMutation({
    mutationFn: ({ id, note = null }: { id: string; note?: string | null }) =>
      commands.ordersSetReady(id, note),
    onSuccess: refresh,
  });
}

/** Serves the order at the till: a ticket with the deposit applied, its receipt queued. */
export function useFulfilOrder() {
  const refresh = useRefresh([...ORDER_MONEY_KEYS, ["sales"]]);
  return useMutation({
    mutationFn: ({ input, printReceipt }: { input: FulfilInput; printReceipt: boolean }) =>
      commands.ordersFulfil(input, printReceipt),
    onSuccess: refresh,
  });
}

/** Serves the order as a validated delivery note or invoice (M3's "Imprimer" prints it). */
export function useDeliverOrder() {
  const refresh = useRefresh([...ORDER_MONEY_KEYS, ["documents"], ["archive"]]);
  return useMutation({
    mutationFn: (input: DeliverInput) => commands.ordersDeliver(input),
    onSuccess: refresh,
  });
}

/** Closes an order not served (refused while a deposit is held). */
export function useCloseOrder() {
  const refresh = useRefresh([qk.orders, ["stock"]]);
  return useMutation({
    mutationFn: ({ id, note = null }: { id: string; note?: string | null }) => commands.ordersClose(id, note),
    onSuccess: refresh,
  });
}

/** The shop keeps the arrhes: they are sold on a ticket whose receipt prints. */
export function useKeepDeposit() {
  const refresh = useRefresh([...ORDER_MONEY_KEYS, ["sales"]]);
  return useMutation({
    mutationFn: ({ id, note = null }: { id: string; note?: string | null }) =>
      commands.ordersKeepDeposit(id, note),
    onSuccess: refresh,
  });
}

/** The appointments of a day (`YYYY-MM-DD`, Algiers). */
export function useAppointmentsDay(date: string) {
  return useQuery({ queryKey: qk.appointmentDay(date), queryFn: () => commands.appointmentsDay(date) });
}

/** The appointments starting in `[fromMs, toMs)`, of one staff member or everyone. */
export function useAppointmentsRange(fromMs: number, toMs: number, staffUserId: string | null = null) {
  return useQuery({
    queryKey: qk.appointmentRange(fromMs, toMs, staffUserId),
    queryFn: () => commands.appointmentsRange(fromMs, toMs, staffUserId),
  });
}

export function useBookAppointment() {
  const refresh = useRefresh([qk.appointments]);
  return useMutation({
    mutationFn: (input: AppointmentInput) => commands.appointmentsBook(input),
    onSuccess: refresh,
  });
}

export function useUpdateAppointment() {
  const refresh = useRefresh([qk.appointments]);
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: AppointmentInput }) =>
      commands.appointmentsUpdate(id, input),
    onSuccess: refresh,
  });
}

export function useSetAppointmentStatus() {
  const refresh = useRefresh([qk.appointments]);
  return useMutation({
    mutationFn: ({ id, status }: { id: string; status: AppointmentStatus }) =>
      commands.appointmentsSetStatus(id, status),
    onSuccess: refresh,
  });
}
