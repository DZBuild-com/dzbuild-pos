import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Lang } from "@/lib/i18n";
import { commands } from "./client";
import { qk } from "./keys";
import type {
  BoardFilter,
  DepositInput,
  IntakeInput,
  Money,
  OwnerType,
  PaymentMethod,
  PickupInput,
  ServiceItemInput,
  ServiceStatus,
} from "./types";

/** Invalidates every query under these keys once a write succeeded. */
function useRefresh(keys: ReadonlyArray<readonly unknown[]>) {
  const client = useQueryClient();
  return () => Promise.all(keys.map((queryKey) => client.invalidateQueries({ queryKey })));
}

/** After money moved for a deposit or a pickup: the tickets, orders, customers, drawer and reports. */
const MONEY_KEYS: ReadonlyArray<readonly unknown[]> = [
  qk.deposits,
  qk.services,
  qk.orders,
  qk.customers,
  ["credit"],
  ["cash"],
  ["reports"],
];

/** A deposit's figures while the cashier types (the duty on its cash comes from the backend). */
export function useDepositQuote(input: DepositInput | null) {
  const idle = input === null || input.amount <= 0;
  return useQuery({
    queryKey: qk.depositQuote(input),
    queryFn: () => commands.depositsQuote(input as DepositInput),
    enabled: !idle,
    placeholderData: idle ? undefined : keepPreviousData,
    staleTime: 0,
  });
}

/** Takes a deposit (its `AC…` receipt prints); on a till it is recorded on the main PC. */
export function useTakeDeposit() {
  const refresh = useRefresh(MONEY_KEYS);
  return useMutation({
    mutationFn: (input: DepositInput) => commands.depositsTake(input),
    onSuccess: refresh,
  });
}

export interface DepositRefund {
  ownerType: OwnerType;
  ownerId: string;
  amount: Money;
  method: PaymentMethod;
  reference: string | null;
}

/** Gives back part of what is held for an order or a ticket, never more (`sales.return`). */
export function useRefundDeposit() {
  const refresh = useRefresh(MONEY_KEYS);
  return useMutation({
    mutationFn: (r: DepositRefund) =>
      commands.depositsRefund(r.ownerType, r.ownerId, r.amount, r.method, r.reference),
    onSuccess: refresh,
  });
}

/** The deposits ledger of an order or a service ticket, oldest first. */
export function useDeposits(ownerType: OwnerType, ownerId: string | null) {
  return useQuery({
    queryKey: qk.depositList(ownerType, ownerId ?? ""),
    queryFn: () => commands.depositsList(ownerType, ownerId ?? ""),
    enabled: ownerId !== null,
  });
}

/** What a scanned or typed voucher reference would spend (`null`: the customer's store credit). */
export function useVoucherDescribe(reference: string | null) {
  const trimmed = reference?.trim() ?? "";
  return useQuery({
    queryKey: qk.depositVoucher(trimmed),
    queryFn: () => commands.vouchersDescribe(trimmed),
    enabled: trimmed !== "",
  });
}

/** Opens a repair or a service ticket; its intake slip prints. */
export function useIntake() {
  const refresh = useRefresh([qk.services]);
  return useMutation({
    mutationFn: (input: IntakeInput) => commands.serviceIntake(input),
    onSuccess: refresh,
  });
}

export function useServiceTicket(id: string | null) {
  return useQuery({
    queryKey: qk.service(id ?? ""),
    queryFn: () => commands.serviceGet(id ?? ""),
    enabled: id !== null,
  });
}

/** The service board, newest first, filtered by status, kind and a text; read again every 30 s
 * (the technicians move tickets from another PC). */
export function useServiceBoard(filter: BoardFilter) {
  return useQuery({
    queryKey: qk.serviceBoard(filter),
    queryFn: () => commands.serviceBoard(filter),
    placeholderData: keepPreviousData,
    refetchInterval: 30_000,
  });
}

export function useSetServiceStatus() {
  const refresh = useRefresh([qk.services]);
  return useMutation({
    mutationFn: ({ id, status, note = null }: { id: string; status: ServiceStatus; note?: string | null }) =>
      commands.serviceSetStatus(id, status, note),
    onSuccess: refresh,
  });
}

export function useAssignTechnician() {
  const refresh = useRefresh([qk.services]);
  return useMutation({
    mutationFn: ({ id, technicianId }: { id: string; technicianId: string | null }) =>
      commands.serviceAssign(id, technicianId),
    onSuccess: refresh,
  });
}

/** Replaces the parts and labour planned on a ticket (sold at pickup). */
export function useSetServiceItems() {
  const refresh = useRefresh([qk.services]);
  return useMutation({
    mutationFn: ({ id, items }: { id: string; items: ServiceItemInput[] }) =>
      commands.serviceSetItems(id, items),
    onSuccess: refresh,
  });
}

/** The "ready" text to copy into an SMS or WhatsApp, in the customer's language. */
export function useReadyMessage(id: string | null, lang: Lang) {
  return useQuery({
    queryKey: qk.readyMessage(id ?? "", lang),
    queryFn: () => commands.serviceReadyMessage(id ?? "", lang),
    enabled: id !== null,
  });
}

/** Hands the item back: its sale with the deposit applied, the ticket's receipt queued. */
export function usePickup() {
  const refresh = useRefresh([...MONEY_KEYS, ["sales"], ["stock"]]);
  return useMutation({
    mutationFn: ({ input, printReceipt }: { input: PickupInput; printReceipt: boolean }) =>
      commands.servicePickup(input, printReceipt),
    onSuccess: refresh,
  });
}

/** Ready tickets waiting longer than the `repairs.uncollected_days` setting. */
export function useUncollected() {
  return useQuery({ queryKey: qk.serviceUncollected, queryFn: () => commands.serviceUncollected() });
}
