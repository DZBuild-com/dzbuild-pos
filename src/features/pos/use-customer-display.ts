import { useEffect, useRef } from "react";
import type { Lang } from "@/lib/i18n";
import { useShowOnDisplay } from "@/lib/ipc/devices";
import type { DisplayFrame, WeightEvent } from "@/lib/ipc/types";
import { useCart } from "./cart-store";
import { useCartPreview } from "./use-cart-preview";

/** What the F11 panel is weighing: the live reading and the chosen product's price per kg. */
export interface Weighing {
  reading: WeightEvent | null;
  unitPrice: number | null;
}

/** A frame waits this long after the last change: a burst of scans or weights sends one. */
const SETTLE_MS = 150;

/** The last frame sent from this window. The feed never sends "idle" over a "paid" frame (the
 * customer screen keeps its thanks for `thanksSeconds`) nor the same frame twice. */
let last: { mode: DisplayFrame["mode"]; json: string } | null = null;

/** Tests start from a clean feed. */
export function resetCustomerDisplayFeed(): void {
  last = null;
}

/** The sale frame the customer screen shows now, if any (HW-042 shows the BaridPay QR over it). */
export function lastDisplayFrame(): DisplayFrame | null {
  return last?.mode === "sale" ? (JSON.parse(last.json) as DisplayFrame) : null;
}

function frameOf(
  lang: Lang,
  weighing: Weighing | null,
  line: DisplayFrame["line"],
  figures: { items: number; total: number; saved: number },
): DisplayFrame {
  const reading = weighing?.reading ?? null;
  return {
    mode: "sale",
    lang,
    line,
    ...figures,
    paid: null,
    change: null,
    weight: reading
      ? {
          net: reading.net,
          stable: reading.stable,
          unitPrice: weighing?.unitPrice ?? null,
          tare: reading.tare,
        }
      : null,
  };
}

/** SAL-047, VRT-010: what the customer screen and the pole display show, from the backend's
 * preview only (never a sum made here): the selected (else last) line, the items, the amount due,
 * the discounts, and the weight while the scale panel is open. */
export function useCustomerDisplayFeed(weighing: Weighing | null): void {
  const show = useShowOnDisplay();
  const preview = useCartPreview();
  const lines = useCart((s) => s.lines);
  const selected = useCart((s) => s.selected);
  const lang = useCart((s) => s.lang);
  const priced = preview.isPlaceholderData ? undefined : preview.data;

  useEffect(() => {
    let frame: DisplayFrame | null = null;
    if (lines.length === 0) {
      frame = weighing?.reading
        ? frameOf(lang, weighing, null, { items: 0, total: 0, saved: 0 })
        : {
            mode: "idle",
            lang,
            line: null,
            items: 0,
            total: 0,
            saved: 0,
            paid: null,
            change: null,
            weight: null,
          };
    } else if (priced) {
      const index = selected ?? priced.cart.lines.length - 1;
      const line = priced.cart.lines[index];
      frame = frameOf(
        lang,
        weighing,
        line
          ? {
              name: line.description,
              latinName: lines[index]?.display.latinName ?? "",
              qty: line.qty,
              unitCode: line.unitCode,
              // what one unit costs the customer, TTC (an HT shop's price is not)
              unitPrice: line.unitPriceTtc,
              total: line.amounts.totalTtc,
            }
          : null,
        {
          items: priced.cart.itemCount,
          total: priced.settlement.totalDue,
          saved: priced.cart.totals.discountTotal,
        },
      );
    }
    if (!frame) return undefined;
    const next = frame;
    const json = JSON.stringify(next);
    // the thanks stay on screen: a sale that ended meanwhile sent its paid frame
    const covers = () => next.mode === "idle" && last?.mode === "paid";
    if (json === last?.json || covers()) return undefined;
    const timer = window.setTimeout(() => {
      if (covers()) return;
      last = { mode: next.mode, json };
      show(next);
    }, SETTLE_MS);
    return () => window.clearTimeout(timer);
  }, [lines, selected, lang, priced, weighing, show]);
}

/** Once, when a sale is done (SaleDoneDialog): the paid amount and the change to hand back. */
export function usePaidFrame(paid: { lang: Lang; total: number; paid: number; change: number } | null): void {
  const show = useShowOnDisplay();
  const sent = useRef(false);
  useEffect(() => {
    if (!paid || sent.current) return;
    sent.current = true;
    const frame: DisplayFrame = {
      mode: "paid",
      lang: paid.lang,
      line: null,
      items: 0,
      total: paid.total,
      saved: 0,
      paid: paid.paid,
      change: paid.change,
      weight: null,
    };
    last = { mode: "paid", json: JSON.stringify(frame) };
    show(frame);
  }, [paid, show]);
}
