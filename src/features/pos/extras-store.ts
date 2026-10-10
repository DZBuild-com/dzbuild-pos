import { create } from "zustand";
import type { BuyerIdentity, PrescriptionInput, SaleExtras } from "@/lib/ipc/types";

/** The van the till sells from (VRT-101): a location of an open delivery round. */
export interface VanSale {
  locationId: string;
  /** The van's name and its round's number, for the till's banner. */
  name: string;
  round: string;
}

interface ExtrasState {
  /** SAL-025: the serials (IMEIs) of each cart line, by the line's index in M1b's cart. */
  serials: Record<number, string[]>;
  /** Pharmacy: one controlled-register entry per controlled line. */
  prescriptions: PrescriptionInput[];
  /** Jewellery: the buyer written in the jeweller register. */
  buyer: BuyerIdentity | null;
  van: VanSale | null;
  setSerials: (line: number, serials: string[]) => void;
  /** The cart removed line `line`: the later lines' extras move up one. */
  lineRemoved: (line: number) => void;
  /** Line `line` now sells `qty` milli-units: serials beyond one per whole piece are dropped, so
   * the checkout does not ask for them again. */
  lineQtyChanged: (line: number, qty: number) => void;
  setPrescriptions: (prescriptions: PrescriptionInput[]) => void;
  setBuyer: (buyer: BuyerIdentity | null) => void;
  setVan: (van: VanSale | null) => void;
  /** With the cart (a new sale); a van sale lasts until "Revenir au magasin". */
  clear: () => void;
  /** The sale's pack part (`SaleRequest.extras`); the payment dialog adds the payment types. */
  toExtras: () => SaleExtras;
}

/** The packs' part of the sale being rung up (M8), beside M1b's cart (`cart-store.ts`), which
 * clears it and tells it which line went. */
export const useExtras = create<ExtrasState>()((set, get) => ({
  serials: {},
  prescriptions: [],
  buyer: null,
  van: null,
  setSerials: (line, serials) => set((s) => ({ serials: { ...s.serials, [line]: serials } })),
  lineRemoved: (line) =>
    set((s) => {
      const serials: Record<number, string[]> = {};
      for (const [key, list] of Object.entries(s.serials)) {
        const i = Number(key);
        if (i < line) serials[i] = list;
        if (i > line) serials[i - 1] = list;
      }
      const prescriptions = s.prescriptions
        .filter((p) => p.line !== line)
        .map((p) => (p.line > line ? { ...p, line: p.line - 1 } : p));
      return { serials, prescriptions };
    }),
  lineQtyChanged: (line, qty) =>
    set((s) => {
      const list = s.serials[line];
      const pieces = Math.max(0, Math.floor(qty / 1_000));
      if (!list || list.length <= pieces) return {};
      const serials = { ...s.serials };
      if (pieces === 0) delete serials[line];
      else serials[line] = list.slice(0, pieces);
      return { serials };
    }),
  setPrescriptions: (prescriptions) => set({ prescriptions }),
  setBuyer: (buyer) => set({ buyer }),
  setVan: (van) => set({ van }),
  clear: () => set({ serials: {}, prescriptions: [], buyer: null }),
  toExtras: () => {
    const s = get();
    return {
      serials: Object.entries(s.serials).flatMap(([line, list]) =>
        list.map((serialNo) => ({ line: Number(line), serialNo })),
      ),
      prescriptions: s.prescriptions,
      buyer: s.buyer ?? undefined,
      paymentTypes: [],
      locationId: s.van?.locationId ?? undefined,
    };
  },
}));
