import type { PaymentInput, PaymentMethod, TenderType } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";

/** Spec §10.5 tender tiles. */
export const TENDER_METHODS: PaymentMethod[] = [
  "cash",
  "card_cib",
  "edahabia",
  "baridimob",
  "cheque",
  "transfer",
  "credit",
  "voucher",
];

/** Tenders on the customer's account (PAY-011/013/020): kridi and store credit, only with a
 * customer chosen (F3). */
export function onAccount(method: PaymentMethod): boolean {
  return method === "credit" || method === "voucher";
}

/** Quick cash notes in centimes (SAL-050). */
export const QUICK_NOTES = [10_000, 20_000, 50_000, 100_000, 200_000] as const;

/** One tender as typed: amounts stay text until sent. */
export interface Tender {
  method: PaymentMethod;
  amount: string;
  reference: string;
  chequeBank: string;
  chequeDate: string;
  /** M8: a custom payment type riding `method` (core Task 16), with its name in the UI language. */
  type?: { id: string; name: string };
}

export function newTender(method: PaymentMethod, amount = ""): Tender {
  return { method, amount, reference: "", chequeBank: "", chequeDate: "" };
}

/** Methods whose number the receipt carries (PAY-008): card authorisation, BaridiMob
 * transaction, cheque number, transfer reference. The account tenders have the customer. */
export function needsReference(method: PaymentMethod): boolean {
  return method !== "cash" && !onAccount(method);
}

/** An amount after a keypad key, cleaned like a typed one: one decimal comma, two decimals at
 * most ("5,5" then "00" stays "5,50", never "5,500", which would read as 5 500 DA). */
export function pressKey(amount: string, key: string): string {
  const [whole = "", ...rest] = `${amount}${key}`.replace(/\./g, ",").split(",");
  return rest.length === 0 ? whole : `${whole},${rest.join("").slice(0, 2)}`;
}

/** A tender `toInputs` drops (no amount typed, or zero). */
function dropped(t: Tender): boolean {
  return t.amount.trim() === "" || parseAmount(t.amount) === 0;
}

/** The custom payment types of the tenders sent (`SaleExtras.paymentTypes`), each by the index of
 * its payment in what `toInputs` sends. */
export function tenderTypes(tenders: Tender[]): TenderType[] {
  return tenders
    .filter((t) => !dropped(t))
    .flatMap((t, payment) => (t.type ? [{ payment, paymentTypeId: t.type.id }] : []));
}

/** The tenders as the backend reads them; empty amounts are dropped; `null` if one is invalid. */
export function toInputs(tenders: Tender[]): PaymentInput[] | null {
  const inputs: PaymentInput[] = [];
  for (const t of tenders) {
    if (dropped(t)) continue;
    const amount = parseAmount(t.amount);
    if (amount === null) return null;
    inputs.push({
      method: t.method,
      amount,
      reference: t.reference.trim() === "" ? null : t.reference.trim(),
      chequeBank: t.method === "cheque" && t.chequeBank.trim() !== "" ? t.chequeBank.trim() : null,
      chequeDate: t.method === "cheque" && t.chequeDate !== "" ? t.chequeDate : null,
    });
  }
  return inputs;
}
