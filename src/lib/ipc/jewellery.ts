import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { Metal, Money, PieceInput } from "./types";

/** After a board or piece change: the jewellery views and the catalogue (live prices). */
function useRefreshJewellery() {
  const client = useQueryClient();
  return () =>
    Promise.all(
      [qk.jewellery, ["products"], ["labels"]].map((queryKey) => client.invalidateQueries({ queryKey })),
    );
}

/** The piece details of a product (`null` when it is not a piece). */
export function useJewelPiece(productId: string | null) {
  return useQuery({
    queryKey: qk.jewelPiece(productId ?? ""),
    queryFn: () => commands.jewelleryPieceGet(productId ?? ""),
    enabled: Boolean(productId),
  });
}

export function useSetJewelPiece() {
  const refresh = useRefreshJewellery();
  return useMutation({
    mutationFn: ({ productId, input }: { productId: string; input: PieceInput }) =>
      commands.jewelleryPieceSet(productId, input),
    onSuccess: refresh,
  });
}

/** Today's price per gram of each metal and fineness. */
export function useMetalBoard() {
  return useQuery({ queryKey: qk.metalBoard, queryFn: () => commands.metalBoard() });
}

export function useMetalHistory(metal: Metal, titre: number) {
  return useQuery({
    queryKey: qk.metalHistory(metal, titre),
    queryFn: () => commands.metalHistory(metal, titre),
  });
}

export function useSetMetalPrice() {
  const refresh = useRefreshJewellery();
  return useMutation({
    mutationFn: ({ metal, titre, pricePerGram }: { metal: Metal; titre: number; pricePerGram: Money }) =>
      commands.metalPriceSet(metal, titre, pricePerGram),
    onSuccess: refresh,
  });
}

/** Writes the live prices of the pieces priced by weight into the catalogue. */
export function useRepriceJewellery() {
  const refresh = useRefreshJewellery();
  return useMutation({ mutationFn: () => commands.jewelleryReprice(), onSuccess: refresh });
}

/** The jeweller register of `year` from entry `fromSeq` (the main PC's on a till). */
export function useJewellerRegister(year: number, fromSeq: number, limit: number) {
  return useQuery({
    queryKey: qk.jewellerRegister(year, fromSeq, limit),
    queryFn: () => commands.jewellerRegisterEntries(year, fromSeq, limit),
  });
}

/** The register's entries read per page (`jeweller_register_entries` returns at most 1000). */
export const REGISTER_PAGE = 500;

/** The jeweller register of `year` in order, a page at a time: the next page starts after the
 * last entry read, while a page came back full. */
export function useJewellerRegisterPages(year: number) {
  return useInfiniteQuery({
    queryKey: [...qk.jewellery, "register", year, "pages"] as const,
    queryFn: ({ pageParam }) => commands.jewellerRegisterEntries(year, pageParam, REGISTER_PAGE),
    initialPageParam: 1,
    getNextPageParam: (last) => {
      const end = last.at(-1);
      return last.length === REGISTER_PAGE && end ? end.seq + 1 : undefined;
    },
  });
}
