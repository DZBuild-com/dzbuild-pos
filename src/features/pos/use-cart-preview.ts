import { useMemo } from "react";
import { useShallow } from "zustand/react/shallow";
import { useSalePreview } from "@/lib/ipc/sales";
import type { PaymentInput } from "@/lib/ipc/types";
import { toCart, useCart } from "./cart-store";

const NO_TENDERS: PaymentInput[] = [];

/** The backend's figures for the cart as it stands (and the tenders being typed); none while
 * `enabled` is false (the payment dialog of a table ticket prices the ticket, not this cart). */
export function useCartPreview(
  tenders: PaymentInput[] = NO_TENDERS,
  storeCredit: number | null = null,
  enabled = true,
) {
  const { lines, priceLevelId, customerId, lang, note } = useCart(
    useShallow((s) => ({
      lines: s.lines,
      priceLevelId: s.priceLevelId,
      customerId: s.customerId,
      lang: s.lang,
      note: s.note,
    })),
  );
  const input = useMemo(
    () =>
      !enabled || lines.length === 0
        ? null
        : { cart: toCart({ lines, priceLevelId, customerId, lang, note }), tenders, storeCredit },
    [enabled, lines, priceLevelId, customerId, lang, note, tenders, storeCredit],
  );
  return useSalePreview(input);
}
