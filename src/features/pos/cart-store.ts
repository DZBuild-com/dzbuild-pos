import { create } from "zustand";
import type { Lang } from "@/lib/i18n";
import type { Cart, CartLine, Discount, ReturnExtras, ReturnInput } from "@/lib/ipc/types";
import { useExtras } from "./extras-store";

/** What the cart shows before the backend prices the line (name, unit, catalogue price). */
export interface LineDisplay {
  name: string;
  unitCode: string;
  unitDecimals: number;
  price: number;
  /** French, else English name: what the pole display shows (Latin text only, HW-043). */
  latinName?: string;
}

export interface DraftLine {
  key: number;
  /** `line.separate`: a weighed package (a scale label, F11), a line of its own never merged
   * into; the backend keeps it with a held ticket. */
  line: CartLine;
  display: LineDisplay;
}

/** A return waiting to pay a new ticket (RET-004): its exact value comes from `returns_quote`. */
export interface ExchangeDraft {
  input: ReturnInput;
  value: number;
  originalNumber: string | null;
  /** M8: the return's pack extras, as quoted (the serials coming back, the exchange voucher). */
  extras?: ReturnExtras;
}

interface CartData {
  lines: DraftLine[];
  selected: number | null;
  priceLevelId: string | null;
  /** The customer chosen with F3: his prices apply, and the credit and store-credit tenders go to
   * his account. */
  customerId: string | null;
  lang: Lang;
  note: string | null;
  /** Milli-units typed before `*`, applied to the next product only. */
  multiplier: number | null;
  lastScan: { name: string; price: number; at: number } | null;
  exchange: ExchangeDraft | null;
  /** The quote or proforma this cart was loaded from (SAL-046): the ticket is linked to it once
   * paid. */
  fromQuote: { id: string; number: string } | null;
}

interface CartActions {
  /** `separate`: a weighed package, always a new line that later scans never merge into. */
  addProduct: (item: { productId: string; qty: number; display: LineDisplay; separate?: boolean }) => void;
  /** Lines of an earlier ticket (SAL-029), at today's prices: no price, discount nor note kept. */
  addLines: (items: { productId: string; qty: number; display: LineDisplay }[]) => void;
  addOpenItem: (item: { description: string; unitPrice: number; taxRateId: string; qty: number }) => void;
  /** A line made elsewhere (a department key's open price, a top-up from `flexy_line`): a line of
   * its own, selected. */
  addLine: (line: CartLine, display: LineDisplay) => void;
  setQty: (index: number, qty: number) => void;
  bump: (index: number, direction: 1 | -1) => void;
  setUnitPrice: (index: number, price: number | null) => void;
  setDiscount: (index: number, discount: Discount) => void;
  discountAll: (discount: Discount) => void;
  setNote: (index: number, note: string | null) => void;
  remove: (index: number) => DraftLine | null;
  select: (index: number | null) => void;
  move: (delta: 1 | -1) => void;
  setMultiplier: (qty: number | null) => void;
  setPriceLevel: (id: string | null) => void;
  setCustomer: (id: string | null) => void;
  setLang: (lang: Lang) => void;
  repeatLast: () => void;
  load: (cart: Cart, displays: LineDisplay[]) => void;
  /** A quote's cart (`documents_offer_cart`), loaded like a recalled ticket and remembered. */
  loadOffer: (cart: Cart, displays: LineDisplay[], quote: { id: string; number: string }) => void;
  startExchange: (draft: ExchangeDraft | null) => void;
  clear: (lang: Lang) => void;
}

export type CartState = CartData & CartActions;

const NONE: Discount = { kind: "none" };
let nextKey = 1;

function plain(line: CartLine): boolean {
  return line.unitPrice === null && line.discount.kind === "none" && line.note === null;
}

/** A plain line of `productId` that a new scan adds its quantity to (a weighed package never). */
function mergeable(draft: DraftLine, productId: string): boolean {
  return !draft.line.separate && draft.line.productId === productId && plain(draft.line);
}

function update(lines: DraftLine[], index: number, change: (line: CartLine) => CartLine): DraftLine[] {
  return lines.map((d, i) => (i === index ? { ...d, line: change(d.line) } : d));
}

/** The selection once line `removed` is gone and `left` lines remain: the line that was selected
 * stays selected; when it is the one removed, the line taking its place (the new last one at the
 * end of the ticket). */
function selectionAfterRemoval(selected: number | null, removed: number, left: number): number | null {
  if (left === 0) return null;
  if (selected === null || selected === removed) return Math.min(removed, left - 1);
  return Math.min(selected > removed ? selected - 1 : selected, left - 1);
}

export const useCart = create<CartState>()((set, get) => ({
  lines: [],
  selected: null,
  priceLevelId: null,
  customerId: null,
  lang: "fr",
  note: null,
  multiplier: null,
  lastScan: null,
  exchange: null,
  fromQuote: null,

  addProduct: ({ productId, qty, display, separate }) => {
    set((s) => {
      const total = s.multiplier === null ? qty : Math.round((s.multiplier * qty) / 1_000);
      const lastScan = { name: display.name, price: display.price, at: Date.now() };
      const index = separate ? -1 : s.lines.findIndex((d) => mergeable(d, productId));
      if (index >= 0) {
        return {
          lines: update(s.lines, index, (l) => ({ ...l, qty: l.qty + total })),
          selected: index,
          multiplier: null,
          lastScan,
        };
      }
      const line: CartLine = {
        productId,
        description: null,
        qty: total,
        unitPrice: null,
        discount: NONE,
        taxRateId: null,
        note: null,
        expectedUnitPrice: display.price,
        ...(separate ? { separate } : {}),
      };
      const lines = [...s.lines, { key: nextKey++, line, display }];
      return { lines, selected: lines.length - 1, multiplier: null, lastScan };
    });
    // NFR-005: the end of the E2E `scan → line` measure (from `dzpos:scan-start`, lib/scanner)
    performance.clearMarks("dzpos:line-added");
    performance.mark("dzpos:line-added");
  },

  addLines: (items) =>
    set((s) => {
      if (items.length === 0) return {};
      let lines = s.lines;
      for (const { productId, qty, display } of items) {
        const index = lines.findIndex((d) => mergeable(d, productId));
        if (index >= 0) {
          lines = update(lines, index, (l) => ({ ...l, qty: l.qty + qty }));
        } else {
          const line: CartLine = {
            productId,
            description: null,
            qty,
            unitPrice: null,
            discount: NONE,
            taxRateId: null,
            note: null,
            expectedUnitPrice: null,
          };
          lines = [...lines, { key: nextKey++, line, display }];
        }
      }
      return { lines, selected: lines.length - 1 };
    }),

  addOpenItem: ({ description, unitPrice, taxRateId, qty }) =>
    set((s) => {
      const line: CartLine = {
        productId: null,
        description,
        qty,
        unitPrice,
        discount: NONE,
        taxRateId,
        note: null,
        expectedUnitPrice: null,
      };
      const display = { name: description, unitCode: "pc", unitDecimals: 0, price: unitPrice };
      const lines = [...s.lines, { key: nextKey++, line, display }];
      return { lines, selected: lines.length - 1 };
    }),

  addLine: (line, display) =>
    set((s) => {
      const lines = [...s.lines, { key: nextKey++, line, display }];
      return { lines, selected: lines.length - 1 };
    }),

  setQty: (index, qty) => {
    if (qty <= 0 || !get().lines[index]) return;
    set((s) => ({ lines: update(s.lines, index, (l) => ({ ...l, qty })) }));
    useExtras.getState().lineQtyChanged(index, qty);
  },

  bump: (index, direction) => {
    const draft = get().lines[index];
    const qty = draft ? draft.line.qty + direction * 1_000 : 0;
    if (qty <= 0) return;
    set((s) => ({ lines: update(s.lines, index, (l) => ({ ...l, qty })) }));
    useExtras.getState().lineQtyChanged(index, qty);
  },

  setUnitPrice: (index, price) =>
    set((s) => ({ lines: update(s.lines, index, (l) => ({ ...l, unitPrice: price })) })),
  setDiscount: (index, discount) =>
    set((s) => ({ lines: update(s.lines, index, (l) => ({ ...l, discount })) })),
  discountAll: (discount) =>
    set((s) => ({ lines: s.lines.map((d) => ({ ...d, line: { ...d.line, discount } })) })),
  setNote: (index, note) => set((s) => ({ lines: update(s.lines, index, (l) => ({ ...l, note })) })),

  remove: (index) => {
    const removed = get().lines[index] ?? null;
    if (!removed) return null;
    set((s) => {
      const lines = s.lines.filter((_, i) => i !== index);
      return { lines, selected: selectionAfterRemoval(s.selected, index, lines.length) };
    });
    useExtras.getState().lineRemoved(index);
    return removed;
  },

  select: (index) => set({ selected: index }),
  move: (delta) =>
    set((s) => {
      if (s.lines.length === 0) return {};
      const from = s.selected ?? (delta > 0 ? -1 : s.lines.length);
      return { selected: Math.max(0, Math.min(s.lines.length - 1, from + delta)) };
    }),
  setMultiplier: (qty) => set({ multiplier: qty }),
  setPriceLevel: (id) => set({ priceLevelId: id }),
  setCustomer: (id) => set({ customerId: id }),
  setLang: (lang) => set({ lang }),

  repeatLast: () =>
    set((s) => {
      const last = s.lines.at(-1);
      if (!last) return {};
      const index = s.lines.length - 1;
      if (last.line.productId !== null && mergeable(last, last.line.productId)) {
        return {
          lines: update(s.lines, index, (l) => ({ ...l, qty: l.qty + last.line.qty })),
          selected: index,
        };
      }
      const lines = [...s.lines, { ...last, key: nextKey++ }];
      return { lines, selected: lines.length - 1 };
    }),

  load: (cart, displays) => {
    useExtras.getState().clear();
    set({
      lines: cart.lines.map((line, i) => ({
        key: nextKey++,
        line,
        display: displays[i] ?? {
          name: line.description ?? "",
          unitCode: "pc",
          unitDecimals: 0,
          price: line.unitPrice ?? 0,
        },
      })),
      // the last line, as after a scan: Del, + and - act on it at once
      selected: cart.lines.length === 0 ? null : cart.lines.length - 1,
      priceLevelId: cart.priceLevelId,
      customerId: cart.customerId,
      lang: cart.lang,
      note: cart.note,
      multiplier: null,
      fromQuote: null,
    });
  },

  loadOffer: (cart, displays, quote) => {
    get().load(cart, displays);
    set({ fromQuote: quote });
  },

  startExchange: (draft) => set({ exchange: draft }),

  clear: (lang) => {
    useExtras.getState().clear();
    set({
      lines: [],
      selected: null,
      priceLevelId: null,
      customerId: null,
      lang,
      note: null,
      multiplier: null,
      lastScan: null,
      exchange: null,
      fromQuote: null,
    });
  },
}));

/** The cart as the backend prices it. */
export function toCart(
  state: Pick<CartData, "lines" | "priceLevelId" | "customerId" | "lang" | "note">,
): Cart {
  return {
    lines: state.lines.map((d) => d.line),
    priceLevelId: state.priceLevelId,
    customerId: state.customerId,
    lang: state.lang,
    note: state.note,
  };
}
