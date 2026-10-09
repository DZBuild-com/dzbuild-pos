import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type {
  AdjustBatchInput,
  CountInput,
  CountMode,
  CountStatus,
  LevelQuery,
  LocationMinimum,
  MovementQuery,
  Qty,
  SerialQuery,
  TransferInput,
  Uncounted,
} from "./types";

/** Invalidates every query under these keys once a write succeeded. */
function useRefresh(keys: ReadonlyArray<readonly unknown[]>) {
  const client = useQueryClient();
  return () => Promise.all(keys.map((queryKey) => client.invalidateQueries({ queryKey })));
}

/** The lots of a product in stock, soonest expiry first. */
export function useLots(productId: string | null) {
  return useQuery({
    queryKey: qk.lots(productId ?? ""),
    queryFn: () => commands.lotsOfProduct(productId ?? ""),
    enabled: productId !== null,
  });
}

/** Lots expired or expiring within `withinDays` (`null`: the `stock.expiry_alert_days` setting). */
export function useExpiry(withinDays: number | null = null) {
  return useQuery({ queryKey: qk.expiry(withinDays), queryFn: () => commands.lotsExpiry(withinDays) });
}

/** A serial search with the filters left out at their defaults. */
export function serialQuery(filter: Partial<SerialQuery>): SerialQuery {
  return { productId: null, status: null, text: "", limit: null, ...filter };
}

export function useSerials(filter: Partial<SerialQuery>, enabled = true) {
  const query = serialQuery(filter);
  return useQuery({
    queryKey: qk.serials(query),
    queryFn: () => commands.serialsList(query),
    enabled,
    placeholderData: keepPreviousData,
  });
}

/** Serial numbers of pieces already on hand (opening stock); receipts carry their own. */
export function useRegisterSerials() {
  const refresh = useRefresh([["stock"]]);
  return useMutation({
    mutationFn: ({
      productId,
      numbers,
      locationId = null,
    }: {
      productId: string;
      numbers: string[];
      locationId?: string | null;
    }) => commands.serialsRegister(productId, numbers, locationId),
    onSuccess: refresh,
  });
}

export function useCounts(status: CountStatus | null = null) {
  return useQuery({ queryKey: qk.countList(status), queryFn: () => commands.countsList(status) });
}

export function useStartCount() {
  const refresh = useRefresh([qk.counts]);
  return useMutation({ mutationFn: (input: CountInput) => commands.countsStart(input), onSuccess: refresh });
}

/** The lines as this user may see them: no expected quantity on a blind count without `stock.adjust`. */
export function useCountLines(id: string | null, pendingOnly = false) {
  return useQuery({
    queryKey: qk.countLines(id ?? "", pendingOnly),
    queryFn: () => commands.countsLines(id ?? "", pendingOnly),
    enabled: id !== null,
    placeholderData: keepPreviousData,
  });
}

/** STK-004: the lots of the count's lot-tracked products (counted, or in stock to look for). */
export function useCountLotLines(id: string) {
  return useQuery({
    queryKey: ["counts", id, "lots"] as const,
    queryFn: () => commands.countsLotLines(id),
    placeholderData: keepPreviousData,
  });
}

/** STK-004: one lot counted by its number (a number the books do not know: a lot found). */
export function useSetCountedLot() {
  const refresh = useRefresh([qk.counts]);
  return useMutation({
    mutationFn: ({
      id,
      productId,
      lotNumber,
      qty,
      mode,
    }: {
      id: string;
      productId: string;
      lotNumber: string;
      qty: Qty;
      mode: CountMode;
    }) => commands.countsSetLot(id, productId, lotNumber, qty, mode),
    onSuccess: refresh,
  });
}

export function useSetCounted() {
  const refresh = useRefresh([qk.counts]);
  return useMutation({
    mutationFn: ({
      id,
      productId,
      qty,
      mode,
    }: {
      id: string;
      productId: string;
      qty: Qty;
      mode: CountMode;
    }) => commands.countsSet(id, productId, qty, mode),
    onSuccess: refresh,
  });
}

/** One scan adds one pack of the code read. */
export function useScanCount() {
  const refresh = useRefresh([qk.counts]);
  return useMutation({
    mutationFn: ({ id, code }: { id: string; code: string }) => commands.countsScan(id, code),
    onSuccess: refresh,
  });
}

/** Posts the variances: the stock moves. */
export function useCloseCount() {
  const refresh = useRefresh([qk.counts, ["stock"], ["reports"]]);
  return useMutation({
    mutationFn: ({ id, uncounted }: { id: string; uncounted: Uncounted }) =>
      commands.countsClose(id, uncounted),
    onSuccess: refresh,
  });
}

export function useCancelCount() {
  const refresh = useRefresh([qk.counts]);
  return useMutation({ mutationFn: (id: string) => commands.countsCancel(id), onSuccess: refresh });
}

export function useTransfer() {
  const refresh = useRefresh([["stock"], ["documents"], ["reports"]]);
  return useMutation({
    mutationFn: (input: TransferInput) => commands.stockTransfer(input),
    onSuccess: refresh,
  });
}

export function useTransfers(limit = 200) {
  return useQuery({ queryKey: qk.transfers(limit), queryFn: () => commands.stockTransfers(limit) });
}

export function useAdjustBatch() {
  const refresh = useRefresh([["stock"], ["reports"]]);
  return useMutation({
    mutationFn: (input: AdjustBatchInput) => commands.stockAdjustBatch(input),
    onSuccess: refresh,
  });
}

export function useMinimums(productId: string | null) {
  return useQuery({
    queryKey: qk.minimums(productId ?? ""),
    queryFn: () => commands.stockMinimumsGet(productId ?? ""),
    enabled: productId !== null,
  });
}

export function useSetMinimums() {
  const refresh = useRefresh([["stock"]]);
  return useMutation({
    mutationFn: ({ productId, minimums }: { productId: string; minimums: LocationMinimum[] }) =>
      commands.stockMinimumsSet(productId, minimums),
    onSuccess: refresh,
  });
}

/** Levels per location with the filters left out at their defaults. */
export function levelQuery(filter: Partial<LevelQuery>): LevelQuery {
  return {
    locationId: null,
    categoryId: null,
    text: "",
    onlyBelowMin: false,
    includeZero: false,
    limit: null,
    ...filter,
  };
}

/** Levels per location; costs come back as 0 without `reports.profit`. */
export function useLevelsView(filter: Partial<LevelQuery>) {
  const query = levelQuery(filter);
  return useQuery({
    queryKey: qk.levelsView(query),
    queryFn: () => commands.stockLevelsView(query),
    placeholderData: keepPreviousData,
  });
}

/** The movement ledger with the filters left out at their defaults; page with `before`/`beforeId`. */
export function movementQuery(filter: Partial<MovementQuery>): MovementQuery {
  return {
    productId: null,
    locationId: null,
    reason: null,
    from: null,
    to: null,
    before: null,
    beforeId: null,
    limit: null,
    ...filter,
  };
}

export function useMovementLog(filter: Partial<MovementQuery>) {
  const query = movementQuery(filter);
  return useQuery({
    queryKey: qk.movementLog(query),
    queryFn: () => commands.stockMovementLog(query),
    placeholderData: keepPreviousData,
  });
}
