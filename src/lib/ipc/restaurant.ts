import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { commands } from "./client";
import { qk } from "./keys";
import type {
  AddLineInput,
  CompReason,
  LineEdit,
  ModifierGroupInput,
  OpenInput,
  PayInput,
  Qty,
  RoomInput,
  SaleExtras,
  SplitBy,
  StationInput,
  TableInput,
  TicketDetails,
  VoidReason,
} from "./types";

/** The floor's minutes since opening and since the last kitchen ticket move on their own; another
 * till's tables change too (RST-002: every 20 s, and after every ticket write). */
const FLOOR_REFRESH_MS = 20_000;

/** Every write on a ticket, the floor or its settings changes what the floor shows. */
function useRefreshDining() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: qk.dining.all });
}

/** The floor of `roomId` (every room with `null`): each table free, occupied or billed. */
export function useDiningBoard(roomId: string | null, enabled = true) {
  return useQuery({
    queryKey: qk.dining.board(roomId),
    queryFn: () => commands.diningBoard(roomId),
    refetchInterval: FLOOR_REFRESH_MS,
    enabled,
  });
}

/** The live tickets without a table: takeaway, delivery, the counter. */
export function useDiningCounter(enabled = true) {
  return useQuery({
    queryKey: qk.dining.counter,
    queryFn: () => commands.diningCounter(),
    refetchInterval: FLOOR_REFRESH_MS,
    enabled,
  });
}

/** One ticket with its lines; `null` waits. */
export function useTicket(id: string | null) {
  return useQuery({
    queryKey: qk.dining.ticket(id ?? ""),
    queryFn: () => commands.diningTicketGet(id ?? ""),
    enabled: id !== null,
  });
}

export function useOpenTicket() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: (input: OpenInput) => commands.diningTicketOpen(input),
    onSuccess: refresh,
  });
}

export function useAddLine() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ ticketId, input }: { ticketId: string; input: AddLineInput }) =>
      commands.diningTicketAddLine(ticketId, input),
    onSuccess: refresh,
  });
}

export function useSetQty() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ lineId, qty }: { lineId: string; qty: Qty }) => commands.diningTicketSetQty(lineId, qty),
    onSuccess: refresh,
  });
}

/** Removes a line the kitchen has not received. */
export function useRemoveUnsent() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: (lineId: string) => commands.diningTicketRemoveUnsent(lineId),
    onSuccess: refresh,
  });
}

/** Moves an unsent line to another seat or course and sets its note. */
export function useEditLine() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ lineId, edit }: { lineId: string; edit: LineEdit }) =>
      commands.diningTicketEditLine(lineId, edit),
    onSuccess: refresh,
  });
}

export function useSetTicketDetails() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ ticketId, details }: { ticketId: string; details: TicketDetails }) =>
      commands.diningTicketSetDetails(ticketId, details),
    onSuccess: refresh,
  });
}

export function useTransferTicket() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ ticketId, tableId }: { ticketId: string; tableId: string }) =>
      commands.diningTicketTransfer(ticketId, tableId),
    onSuccess: refresh,
  });
}

export function useMergeTickets() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ fromId, intoId }: { fromId: string; intoId: string }) =>
      commands.diningTicketMerge(fromId, intoId),
    onSuccess: refresh,
  });
}

/** Sends what the kitchen has not received; the backend queues the kitchen tickets' print. */
export function useSendToKitchen() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: (ticketId: string) => commands.diningTicketSend(ticketId),
    onSuccess: refresh,
  });
}

/** Voids `qty` of a line (all of it with `null`) with a reason. */
export function useVoidLine() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ lineId, qty, reason }: { lineId: string; qty: Qty | null; reason: VoidReason }) =>
      commands.diningTicketVoidLine(lineId, qty, reason),
    onSuccess: refresh,
  });
}

export function useVoidTicket() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ ticketId, reason }: { ticketId: string; reason: VoidReason }) =>
      commands.diningTicketVoid(ticketId, reason),
    onSuccess: refresh,
  });
}

export function useCompLine() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ lineId, reason }: { lineId: string; reason: CompReason }) =>
      commands.diningTicketCompLine(lineId, reason),
    onSuccess: refresh,
  });
}

/** The addition (printed by the backend); `parts` asks for even shares. (The till's print
 * failures have their own copy in `dining-print.ts`: the /pos chunk carries no restaurant hook.) */
export function usePrebill() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ ticketId, parts = null }: { ticketId: string; parts?: number | null }) =>
      commands.diningTicketPrebill(ticketId, parts),
    onSuccess: refresh,
  });
}

export function useSplitTicket() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ ticketId, by }: { ticketId: string; by: SplitBy }) =>
      commands.diningTicketSplit(ticketId, by),
    onSuccess: refresh,
  });
}

/** Pays the ticket: a validated sale (with the till's pack extras), its receipt queued. */
export function usePayTicket() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({
      input,
      printReceipt,
      extras = null,
    }: {
      input: PayInput;
      printReceipt: boolean;
      extras?: SaleExtras | null;
    }) => commands.diningTicketPay(input, printReceipt, extras),
    onSuccess: () =>
      Promise.all([
        client.invalidateQueries({ queryKey: qk.dining.all }),
        client.invalidateQueries({ queryKey: ["cash"] }),
        client.invalidateQueries({ queryKey: ["stock"] }),
      ]),
  });
}

/** Prints a kitchen ticket again (one whose print failed). */
export function useKitchenReprint() {
  return useMutation({ mutationFn: (id: string) => commands.kitchenReprint(id) });
}

export function useRooms(enabled = true) {
  return useQuery({ queryKey: qk.dining.rooms, queryFn: () => commands.diningRooms(), enabled });
}

export function useSaveRoom() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ id, input }: { id: string | null; input: RoomInput }) =>
      commands.diningRoomSave(id, input),
    onSuccess: refresh,
  });
}

export function useArchiveRoom() {
  const refresh = useRefreshDining();
  return useMutation({ mutationFn: (id: string) => commands.diningRoomArchive(id), onSuccess: refresh });
}

export function useTables(roomId: string | null, enabled = true) {
  return useQuery({
    queryKey: qk.dining.tables(roomId),
    queryFn: () => commands.diningTables(roomId),
    enabled,
  });
}

/** Creates a table (`id: null`) or changes one, its cell included. */
export function useSaveTable() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ id, input }: { id: string | null; input: TableInput }) =>
      commands.diningTableSave(id, input),
    onSuccess: refresh,
  });
}

export function useArchiveTable() {
  const refresh = useRefreshDining();
  return useMutation({ mutationFn: (id: string) => commands.diningTableArchive(id), onSuccess: refresh });
}

export function useModifierGroups(enabled = true) {
  return useQuery({ queryKey: qk.dining.modifiers, queryFn: () => commands.modifierGroups(), enabled });
}

export function useSaveModifierGroup() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ id, input }: { id: string | null; input: ModifierGroupInput }) =>
      commands.modifierGroupSave(id, input),
    onSuccess: refresh,
  });
}

export function useArchiveModifierGroup() {
  const refresh = useRefreshDining();
  return useMutation({ mutationFn: (id: string) => commands.modifierGroupArchive(id), onSuccess: refresh });
}

/** The groups a product asks at the till; `null` waits. */
export function useModifiersFor(productId: string | null) {
  return useQuery({
    queryKey: qk.dining.modifiersOf(productId ?? ""),
    queryFn: () => commands.modifiersForProduct(productId ?? ""),
    enabled: productId !== null,
  });
}

/** The groups a product asks, read when it is tapped at the till (the cache of `useModifiersFor`). */
export function useFetchModifiers() {
  const client = useQueryClient();
  return useCallback(
    (productId: string) =>
      client.fetchQuery({
        queryKey: qk.dining.modifiersOf(productId),
        queryFn: () => commands.modifiersForProduct(productId),
      }),
    [client],
  );
}

export function useSetProductModifiers() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ productId, groupIds }: { productId: string; groupIds: string[] }) =>
      commands.modifiersSetProduct(productId, groupIds),
    onSuccess: refresh,
  });
}

export function useStations(enabled = true) {
  return useQuery({ queryKey: qk.dining.stations, queryFn: () => commands.kitchenStations(), enabled });
}

export function useSaveStation() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ id, input }: { id: string | null; input: StationInput }) =>
      commands.kitchenStationSave(id, input),
    onSuccess: refresh,
  });
}

export function useArchiveStation() {
  const refresh = useRefreshDining();
  return useMutation({ mutationFn: (id: string) => commands.kitchenStationArchive(id), onSuccess: refresh });
}

/** The products of the daily menu of `day` (`YYYY-MM-DD`). */
export function useDailyMenu(day: string, enabled = true) {
  return useQuery({ queryKey: qk.dining.menu(day), queryFn: () => commands.dailyMenuGet(day), enabled });
}

export function useSetDailyMenu() {
  const refresh = useRefreshDining();
  return useMutation({
    mutationFn: ({ day, productIds }: { day: string; productIds: string[] }) =>
      commands.dailyMenuSet(day, productIds),
    onSuccess: refresh,
  });
}
