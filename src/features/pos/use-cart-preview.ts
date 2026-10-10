import { useMemo, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useSalePreview } from "@/lib/ipc/sales";
import type { PaymentInput, PaymentPreview } from "@/lib/ipc/types";
import { type DraftLine, toCart, useCart } from "./cart-store";

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

export type PricedCartLine = PaymentPreview["cart"]["lines"][number];

interface Owned {
  data: PaymentPreview;
  /** The keys of the draft lines `data` was computed for, in its line order. */
  keys: number[];
}

function sameKeys(a: readonly number[], b: readonly number[]): boolean {
  return a.length === b.length && a.every((key, i) => key === b[i]);
}

/**
 * The backend's line for each of `lines`, matched by the draft line's key rather than its
 * position. While the cart as it now stands is being priced, the preview on screen is the previous
 * cart's: it still prices the lines it was computed for, and a line added or moved since gets
 * nothing ("…"). Without this, voiding line 1 would show its name and total on the line that took
 * its place until the new figures arrive.
 */
export function usePricedLines(
  lines: readonly DraftLine[],
  preview: { data: PaymentPreview | undefined; isPlaceholderData: boolean },
): (PricedCartLine | undefined)[] {
  const keys = lines.map((d) => d.key);
  const fresh = preview.isPlaceholderData ? undefined : preview.data;
  // the figures last computed for the cart on screen, remembered with that cart's line keys
  const [owned, setOwned] = useState<Owned | null>(null);
  if (fresh !== undefined && (owned?.data !== fresh || !sameKeys(owned.keys, keys))) {
    setOwned({ data: fresh, keys });
  }
  const source: Owned | null =
    fresh !== undefined
      ? { data: fresh, keys }
      : owned !== null && owned.data === preview.data
        ? owned
        : null;
  return lines.map((d) => {
    const at = source ? source.keys.indexOf(d.key) : -1;
    return at >= 0 ? source?.data.cart.lines[at] : undefined;
  });
}
