import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Lang } from "@/lib/i18n";
import { commands } from "./client";
import { qk } from "./keys";
import { levelQuery } from "./stock-m2";
import type {
  Money,
  PackInput,
  Qty,
  RoundFilter,
  RoundInput,
  StopReport,
  TransferLine,
  VanNames,
} from "./types";

/** Invalidates every query under these keys once a write succeeded. */
function useRefresh(keys: ReadonlyArray<readonly unknown[]>) {
  const client = useQueryClient();
  return () => Promise.all(keys.map((queryKey) => client.invalidateQueries({ queryKey })));
}

/** A customer's agreed prices, with the catalogue price beside each. */
export function useCustomerPrices(customerId: string | null) {
  return useQuery({
    queryKey: qk.customerPrices(customerId ?? ""),
    queryFn: () => commands.wholesalePrices(customerId ?? ""),
    enabled: customerId !== null,
  });
}

/** Sets a customer's agreed price of a product (`price: null` removes it). */
export function useSetCustomerPrice() {
  const refresh = useRefresh([qk.wholesale, ["sales"]]);
  return useMutation({
    mutationFn: ({
      customerId,
      productId,
      price,
    }: {
      customerId: string;
      productId: string;
      price: Money | null;
    }) => commands.wholesalePriceSet(customerId, productId, price),
    onSuccess: refresh,
  });
}

/** A product's minimum order (`null`: none); idle without a product. */
export function useMinOrder(productId: string | null) {
  return useQuery({
    queryKey: qk.minOrder(productId ?? ""),
    queryFn: () => commands.wholesaleMinOrder(productId ?? ""),
    enabled: productId !== null,
  });
}

/** Sets a product's minimum order (`null` removes it). */
export function useSetMinOrder() {
  const refresh = useRefresh([qk.wholesale, ["products"]]);
  return useMutation({
    mutationFn: ({ productId, minOrderQty }: { productId: string; minOrderQty: Qty | null }) =>
      commands.wholesaleMinOrderSet(productId, minOrderQty),
    onSuccess: refresh,
  });
}

/** A pack (carton, bag, pallet…) of a unit product: a product of the catalogue. */
export function useCreatePack() {
  const refresh = useRefresh([["products"], ["catalog"]]);
  return useMutation({
    mutationFn: (input: PackInput) => commands.wholesalePackCreate(input),
    onSuccess: refresh,
  });
}

export function useRounds(filter: RoundFilter) {
  return useQuery({
    queryKey: qk.roundList(filter),
    queryFn: () => commands.roundsList(filter),
    placeholderData: keepPreviousData,
  });
}

export function useRound(id: string | null) {
  return useQuery({
    queryKey: qk.round(id ?? ""),
    queryFn: () => commands.roundsGet(id ?? ""),
    enabled: id !== null,
  });
}

export function useCreateRound() {
  const refresh = useRefresh([qk.rounds]);
  return useMutation({ mutationFn: (input: RoundInput) => commands.roundsCreate(input), onSuccess: refresh });
}

/** The round's stops in route order (a planned round only). */
export function useSetStops() {
  const refresh = useRefresh([qk.rounds]);
  return useMutation({
    mutationFn: ({ id, documentIds }: { id: string; documentIds: string[] }) =>
      commands.roundsSetStops(id, documentIds),
    onSuccess: refresh,
  });
}

export function useStartRound() {
  const refresh = useRefresh([qk.rounds]);
  return useMutation({ mutationFn: (id: string) => commands.roundsStart(id), onSuccess: refresh });
}

/** What happened at a stop: delivered or not, what was collected and how. */
export function useReportStop() {
  const refresh = useRefresh([qk.rounds]);
  return useMutation({
    mutationFn: ({ id, report }: { id: string; report: StopReport }) => commands.roundsReportStop(id, report),
    onSuccess: refresh,
  });
}

/** What closing the round will book (collected by method, the cash to hand in, the stops not
 * delivered), read afresh each time the confirmation opens; idle with `null`. */
export function useClosePreview(id: string | null) {
  return useQuery({
    queryKey: qk.closePreview(id ?? ""),
    queryFn: () => commands.roundsClosePreview(id ?? ""),
    enabled: id !== null,
    staleTime: 0,
    gcTime: 0,
  });
}

/** What a van holds: its stock location's levels; idle without a van. */
export function useVanStock(locationId: string | null) {
  const query = levelQuery({ locationId });
  return useQuery({
    queryKey: qk.levelsView(query),
    queryFn: () => commands.stockLevelsView(query),
    enabled: locationId !== null,
  });
}

/** Closes the round: one versement per collection, every `RV…` receipt queued. */
export function useCloseRound() {
  const refresh = useRefresh([qk.rounds, qk.customers, ["credit"], ["cash"], ["reports"]]);
  return useMutation({ mutationFn: (id: string) => commands.roundsClose(id), onSuccess: refresh });
}

/** The route sheet's page for the in-app preview; `null` asks nothing. */
export function useRouteSheet(id: string | null, lang: Lang) {
  return useQuery({
    queryKey: qk.routeSheet(id ?? "", lang),
    queryFn: () => commands.roundsRouteSheet(id ?? "", lang),
    enabled: id !== null,
  });
}

/** Prints the route sheet on this PC's A4 printer through the print host. */
export function usePrintRouteSheet() {
  return useMutation({
    mutationFn: ({ id, lang }: { id: string; lang: Lang }) => commands.roundsRouteSheetPrint(id, lang),
  });
}

/** A delivery van: a stock location of its own. */
export function useCreateVan() {
  const refresh = useRefresh([["stock"]]);
  return useMutation({
    mutationFn: ({ code, names }: { code: string; names: VanNames }) => commands.roundsCreateVan(code, names),
    onSuccess: refresh,
  });
}

/** Loads the round's van from the default location (an M2 transfer). */
export function useLoadVan() {
  const refresh = useRefresh([qk.rounds, ["stock"], ["documents"]]);
  return useMutation({
    mutationFn: ({ id, lines }: { id: string; lines: TransferLine[] }) => commands.roundsLoad(id, lines),
    onSuccess: refresh,
  });
}

/** Brings what is left on the round's van back (`null` when it was empty). */
export function useUnloadVan() {
  const refresh = useRefresh([qk.rounds, ["stock"], ["documents"]]);
  return useMutation({ mutationFn: (id: string) => commands.roundsUnload(id), onSuccess: refresh });
}
