import type { DocType, Permission } from "@/lib/ipc/types";

/** The commercial kinds and the permission each needs (pos-core `billing::kind_permission`). */
const BILLING_KINDS: readonly (readonly [DocType, Permission])[] = [
  ["invoice", "documents.invoice"],
  ["quote", "documents.quote"],
  ["proforma", "documents.quote"],
  ["delivery_note", "documents.delivery"],
  ["credit_note", "documents.credit_note"],
];

/** The kinds the archive lists for these permissions, as the backend's `visible_kinds` filters
 * them: the commercial kinds of the user's permissions, tickets and return notes with
 * `sales.view_all`, supplier documents with `purchases.view`, transfer notes with
 * `stock.transfer`. */
export function visibleKinds(permissions: readonly Permission[]): DocType[] {
  const has = (p: Permission) => permissions.includes(p);
  const kinds = BILLING_KINDS.filter(([, p]) => has(p)).map(([kind]) => kind);
  if (has("sales.view_all")) kinds.push("ticket", "return_note");
  if (has("purchases.view")) kinds.push("purchase_order", "goods_receipt", "supplier_invoice");
  if (has("stock.transfer")) kinds.push("transfer");
  return kinds;
}

/** Whether these permissions print, save as PDF and preview a document of `kind`, as the
 * backend's `require_output` checks it: the kind's own permission (tickets and return notes
 * `sales.reprint`, supplier documents `purchases.view`, a transfer note `stock.transfer` and, for
 * its prices at cost, `reports.profit`). */
export function mayOutput(kind: DocType, permissions: readonly Permission[]): boolean {
  const has = (p: Permission) => permissions.includes(p);
  const billing = BILLING_KINDS.find(([k]) => k === kind);
  if (billing) return has(billing[1]);
  switch (kind) {
    case "ticket":
    case "return_note":
      return has("sales.reprint");
    case "purchase_order":
    case "goods_receipt":
    case "supplier_invoice":
      return has("purchases.view");
    default:
      return has("stock.transfer") && has("reports.profit");
  }
}

/** The kinds typed at the desk ("New"): a credit note is made from its invoice, never from scratch. */
export function newKinds(permissions: readonly Permission[]): DocType[] {
  return BILLING_KINDS.filter(([kind, p]) => kind !== "credit_note" && permissions.includes(p)).map(
    ([kind]) => kind,
  );
}
