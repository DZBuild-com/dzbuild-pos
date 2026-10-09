import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { commands } from "./client";
import type { CartLine, Customer, PaymentInput, Piece, VoucherInfo } from "./types";

// The till's pack extensions (Task 24, a lazy chunk): the pack reads and writes that the
// sale screen's pack dialogs call, through the generated bindings.

/** LOY-002: the points the ticket being paid earns `customerId` (the backend's rule, the one the
 * sale's earn row follows); `null` without a live card or with the loyalty module off. Idle until
 * the ticket's total is known. */
export function usePointsPreview(customerId: string, totalTtc: number | undefined, payments: PaymentInput[]) {
  return useQuery({
    queryKey: ["till", "points", customerId, totalTtc, payments] as const,
    queryFn: () => commands.salesPointsPreview(customerId, totalTtc ?? 0, payments),
    enabled: totalTtc !== undefined,
    placeholderData: keepPreviousData,
  });
}

/** The vouchers a reference spends (SAL-061, LOY-001, ORD): `null` when nothing matches. */
export function describeVoucher(reference: string): Promise<VoucherInfo | null> {
  return commands.vouchersDescribe(reference);
}

/** The customer whose loyalty card this is (`null` when no live card has the code). */
export function customerOfCard(code: string): Promise<Customer | null> {
  return commands.loyaltyCardLookup(code);
}

/** A top-up line of `amount` centimes to `phone` (the operator's service product). */
export function flexyLine(operator: string, phone: string, amount: number): Promise<CartLine> {
  return commands.flexyLine(operator, phone, amount);
}

/** The product's precious-metal piece (the jeweller register's buyer is asked), `null` if none. */
export function jewelPiece(productId: string): Promise<Piece | null> {
  return commands.jewelleryPieceGet(productId);
}

export function useSerialsAvailable(productId: string) {
  return useQuery({
    queryKey: ["till", "serials", productId],
    queryFn: () => commands.serialsAvailable(productId),
  });
}

export function useFlexyOperators() {
  return useQuery({ queryKey: ["till", "flexyOperators"], queryFn: () => commands.flexyOperators() });
}

/** A variant model's grid at `locationId` (a van's stock when selling from one). */
export function useVariantGrid(parentId: string, locationId: string | null) {
  return useQuery({
    queryKey: ["till", "variantGrid", parentId, locationId],
    queryFn: () => commands.variantGrid(parentId, locationId, null),
  });
}

/** The shop's BaridPay QR (HW-042), `null` when none is set. */
export function useBaridpayQr(enabled: boolean) {
  return useQuery({
    queryKey: ["till", "baridpayQr"],
    queryFn: () => commands.paymentsBaridpayQr(),
    enabled,
    staleTime: 60_000,
  });
}

/** The rounds on the road between `from` and `to` (`YYYY-MM-DD`): the vans a till can sell from. */
export function useRoundsOut(from: string, to: string) {
  return useQuery({
    queryKey: ["till", "rounds", from, to],
    queryFn: () => commands.roundsList({ status: "out", from, to }),
  });
}
