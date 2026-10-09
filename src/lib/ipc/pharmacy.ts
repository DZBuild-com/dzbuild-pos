import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { ChifaMapping, PharmacyProductInput } from "./types";

function useRefreshPharmacy() {
  const client = useQueryClient();
  return () =>
    Promise.all([
      client.invalidateQueries({ queryKey: qk.pharmacy.all }),
      client.invalidateQueries({ queryKey: ["products"] }),
    ]);
}

/** A medicine's pharmacy row (DCI, controlled, schedule); `null` waits. */
export function usePharmacyProduct(productId: string | null) {
  return useQuery({
    queryKey: qk.pharmacy.product(productId ?? ""),
    queryFn: () => commands.pharmacyProductGet(productId ?? ""),
    enabled: productId !== null,
  });
}

export function useSetPharmacyProduct() {
  const refresh = useRefreshPharmacy();
  return useMutation({
    mutationFn: ({ productId, input }: { productId: string; input: PharmacyProductInput }) =>
      commands.pharmacyProductSet(productId, input),
    onSuccess: refresh,
  });
}

/** The generics of a medicine, cheapest first (the substitution notice); `null` waits. */
export function useEquivalents(productId: string | null, levelId: string | null = null) {
  return useQuery({
    queryKey: qk.pharmacy.equivalents(productId ?? "", levelId),
    queryFn: () => commands.pharmacyEquivalents(productId ?? "", levelId),
    enabled: productId !== null,
  });
}

/** Lots expired and expiring within `withinDays` (`null`: the shop's alert setting). */
export function useExpiryBoard(withinDays: number | null = null) {
  return useQuery({
    queryKey: qk.pharmacy.expiry(withinDays),
    queryFn: () => commands.pharmacyExpiryBoard(withinDays),
  });
}

/** The dry run of the Chifa list opened with the import dialog (its token). */
export function useChifaCheck() {
  return useMutation({
    mutationFn: ({ token, mapping }: { token: string; mapping: ChifaMapping }) =>
      commands.chifaCheck(token, mapping),
  });
}

/** Writes the checked list (refused as a whole when a row has a problem). */
export function useChifaApply() {
  const refresh = useRefreshPharmacy();
  return useMutation({
    mutationFn: ({ token, mapping }: { token: string; mapping: ChifaMapping }) =>
      commands.chifaApply(token, mapping),
    onSuccess: refresh,
  });
}

export function useChifaList(text: string, limit = 200) {
  return useQuery({
    queryKey: qk.pharmacy.chifa(text, limit),
    queryFn: () => commands.chifaList(text, limit),
  });
}

/** A medicine's Chifa reference price; `null` waits. */
export function useChifaOf(productId: string | null) {
  return useQuery({
    queryKey: qk.pharmacy.chifaOf(productId ?? ""),
    queryFn: () => commands.chifaOfProduct(productId ?? ""),
    enabled: productId !== null,
  });
}

/** The controlled-medicine register of `from`..`to` (`YYYY-MM-DD`, `pharmacy.register` only). */
export function useRegisterEntries(from: string, to: string, enabled = true) {
  return useQuery({
    queryKey: qk.pharmacy.register(from, to),
    queryFn: () => commands.controlledRegisterEntries(from, to),
    enabled,
  });
}
