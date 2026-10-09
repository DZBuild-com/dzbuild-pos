import { createStore, type StoreApi } from "zustand";
import { isLang, type Lang } from "@/lib/i18n";
import type { CartLine, Discount, DocType, DraftCart, DraftInput } from "@/lib/ipc/types";

export interface EditorLine {
  key: number;
  line: CartLine;
  name: string;
  unitCode: string;
}

interface EditorData {
  docType: DocType;
  customerId: string | null;
  lines: EditorLine[];
  lang: Lang;
  secondLang: Lang | null;
  validUntil: string | null;
  dueDate: string | null;
  note: string | null;
  priceLevelId: string | null;
}

interface EditorActions {
  addProduct: (item: { productId: string; qty: number; name: string; unitCode: string }) => void;
  addOpenItem: (item: { description: string; unitPrice: number; taxRateId: string; qty: number }) => void;
  setQty: (key: number, qty: number) => void;
  setUnitPrice: (key: number, price: number | null) => void;
  setDiscount: (key: number, discount: Discount) => void;
  setLineNote: (key: number, note: string | null) => void;
  remove: (key: number) => void;
  setCustomer: (id: string | null) => void;
  setLang: (lang: Lang) => void;
  setSecondLang: (lang: Lang | null) => void;
  setValidUntil: (day: string | null) => void;
  setDueDate: (day: string | null) => void;
  setNote: (note: string | null) => void;
  loadDraft: (draft: DraftCart, names: Record<string, { name: string; unitCode: string }>) => void;
}

export type EditorState = EditorData & EditorActions;

const NONE: Discount = { kind: "none" };
let nextKey = 1;

function productLine(productId: string, qty: number): CartLine {
  return {
    productId,
    description: null,
    qty,
    unitPrice: null,
    discount: NONE,
    taxRateId: null,
    note: null,
    expectedUnitPrice: null,
  };
}

/** A catalogue line at its price, without discount or note: the same product scanned again adds
 * to it. */
function plain(line: CartLine): boolean {
  return line.unitPrice === null && line.discount.kind === "none" && line.note === null;
}

/** DOC-034: the second language Réglages › Documents sets for new documents
 * (`documents.layout.secondLang`), unless it is the document's own language. */
export function defaultSecondLang(layout: unknown, lang: Lang): Lang | null {
  const second =
    typeof layout === "object" && layout !== null ? (layout as { secondLang?: unknown }).secondLang : null;
  return typeof second === "string" && isLang(second) && second !== lang ? second : null;
}

/** The lines, customer, languages and dates of a draft being typed at the desk (DOC-001): what
 * the backend prices and stores, never an amount. A new document starts bilingual when the
 * settings ask for it (`secondLang`). */
export function createEditorStore(
  docType: DocType,
  lang: Lang,
  secondLang: Lang | null = null,
): StoreApi<EditorState> {
  return createStore<EditorState>()((set) => {
    const change = (key: number, update: (line: CartLine) => CartLine) =>
      set((s) => ({ lines: s.lines.map((l) => (l.key === key ? { ...l, line: update(l.line) } : l)) }));
    return {
      docType,
      customerId: null,
      lines: [],
      lang,
      secondLang: secondLang === lang ? null : secondLang,
      validUntil: null,
      dueDate: null,
      note: null,
      priceLevelId: null,
      addProduct: ({ productId, qty, name, unitCode }) =>
        set((s) => {
          const same = s.lines.find((l) => l.line.productId === productId && plain(l.line));
          if (same) {
            return {
              lines: s.lines.map((l) =>
                l === same ? { ...l, line: { ...l.line, qty: l.line.qty + qty } } : l,
              ),
            };
          }
          return {
            lines: [...s.lines, { key: nextKey++, line: productLine(productId, qty), name, unitCode }],
          };
        }),
      addOpenItem: ({ description, unitPrice, taxRateId, qty }) =>
        set((s) => ({
          lines: [
            ...s.lines,
            {
              key: nextKey++,
              line: { ...productLine("", qty), productId: null, description, unitPrice, taxRateId },
              name: description,
              unitCode: "pc",
            },
          ],
        })),
      setQty: (key, qty) => change(key, (l) => ({ ...l, qty })),
      setUnitPrice: (key, unitPrice) => change(key, (l) => ({ ...l, unitPrice })),
      setDiscount: (key, discount) => change(key, (l) => ({ ...l, discount })),
      setLineNote: (key, note) => change(key, (l) => ({ ...l, note })),
      remove: (key) => set((s) => ({ lines: s.lines.filter((l) => l.key !== key) })),
      setCustomer: (customerId) => set({ customerId }),
      setLang: (next) =>
        set((s) => ({ lang: next, secondLang: s.secondLang === next ? null : s.secondLang })),
      setSecondLang: (second) => set((s) => ({ secondLang: second === s.lang ? null : second })),
      setValidUntil: (validUntil) => set({ validUntil }),
      setDueDate: (dueDate) => set({ dueDate }),
      setNote: (note) => set({ note }),
      loadDraft: (draft, names) =>
        set({
          docType: draft.docType,
          customerId: draft.cart.customerId,
          lang: draft.cart.lang,
          secondLang: draft.secondLang,
          validUntil: draft.validUntil,
          dueDate: draft.dueDate,
          note: draft.cart.note,
          priceLevelId: draft.cart.priceLevelId,
          lines: draft.cart.lines.map((line) => {
            const known = line.productId ? names[line.productId] : undefined;
            return {
              key: nextKey++,
              line,
              name: known?.name ?? line.description ?? "",
              unitCode: known?.unitCode ?? "pc",
            };
          }),
        }),
    };
  });
}

/** The draft the backend prices and stores; the editor never computes an amount. */
export function toInput(s: EditorData): DraftInput {
  return {
    docType: s.docType,
    cart: {
      lines: s.lines.map((l) => l.line),
      priceLevelId: s.priceLevelId,
      customerId: s.customerId,
      lang: s.lang,
      note: s.note,
    },
    validUntil: s.validUntil,
    dueDate: s.dueDate,
    secondLang: s.secondLang,
  };
}
