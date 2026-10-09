import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { SellInput, VoucherFilter } from "./types";

/** After a points or voucher write: the accounts, the vouchers report and the drawer (a gift
 * voucher's cash). */
function useRefreshLoyalty() {
  const client = useQueryClient();
  return () =>
    Promise.all(
      [qk.loyalty, qk.vouchers, ["cash"], ["reports"]].map((queryKey) =>
        client.invalidateQueries({ queryKey }),
      ),
    );
}

/** The customer of a scanned loyalty card (`null` for another code); idle for an empty code. */
export function useLoyaltyCard(code: string | null) {
  return useQuery({
    queryKey: qk.loyaltyCard(code ?? ""),
    queryFn: () => commands.loyaltyCardLookup(code ?? ""),
    enabled: Boolean(code),
  });
}

/** The points of a customer as of today (balance, expiring soon, redeemable). */
export function useLoyaltyAccount(customerId: string | null) {
  return useQuery({
    queryKey: qk.loyaltyAccount(customerId ?? ""),
    queryFn: () => commands.loyaltyAccount(customerId ?? ""),
    enabled: Boolean(customerId),
  });
}

export function useLoyaltyEntries(customerId: string | null) {
  return useQuery({
    queryKey: qk.loyaltyEntries(customerId ?? ""),
    queryFn: () => commands.loyaltyEntries(customerId ?? ""),
    enabled: Boolean(customerId),
  });
}

export function useEnrol() {
  const refresh = useRefreshLoyalty();
  return useMutation({
    mutationFn: ({ customerId, code }: { customerId: string; code: string | null }) =>
      commands.loyaltyEnrol(customerId, code),
    onSuccess: refresh,
  });
}

export function useBlockCard() {
  const refresh = useRefreshLoyalty();
  return useMutation({ mutationFn: (code: string) => commands.loyaltyBlock(code), onSuccess: refresh });
}

/** Points turned into a loyalty voucher; its slip prints on this PC. */
export function useRedeemPoints() {
  const refresh = useRefreshLoyalty();
  return useMutation({
    mutationFn: ({ customerId, points }: { customerId: string; points: number }) =>
      commands.loyaltyRedeem(customerId, points),
    onSuccess: refresh,
  });
}

export function useAdjustPoints() {
  const refresh = useRefreshLoyalty();
  return useMutation({
    mutationFn: ({ customerId, points, note }: { customerId: string; points: number; note: string }) =>
      commands.loyaltyAdjust(customerId, points, note),
    onSuccess: refresh,
  });
}

/** The duty on the cash and what to receive for a gift voucher, from the backend; idle without
 * an amount. */
export function useVoucherQuote(input: SellInput | null) {
  const idle = input === null || input.amount <= 0;
  return useQuery({
    queryKey: qk.voucherQuote(input),
    queryFn: () => commands.vouchersQuoteSale(input as SellInput),
    enabled: !idle,
    placeholderData: idle ? undefined : keepPreviousData,
    staleTime: 0,
  });
}

/** A gift voucher sold; its slip prints on this PC and the drawer opens for its cash. */
export function useSellVoucher() {
  const refresh = useRefreshLoyalty();
  return useMutation({ mutationFn: (input: SellInput) => commands.vouchersSell(input), onSuccess: refresh });
}

/** A voucher by its code or scanned barcode (the tender's reference); idle for an empty code. */
export function useVoucher(code: string | null) {
  return useQuery({
    queryKey: qk.voucher(code ?? ""),
    queryFn: () => commands.vouchersGet(code ?? ""),
    enabled: Boolean(code),
    staleTime: 0,
  });
}

export function useVouchers(filter: VoucherFilter) {
  return useQuery({ queryKey: qk.voucherList(filter), queryFn: () => commands.vouchersList(filter) });
}

export function useReprintVoucher() {
  return useMutation({ mutationFn: (code: string) => commands.vouchersReprint(code) });
}
