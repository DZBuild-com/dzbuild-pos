import type { DueState, GuaranteeKind, PaymentMethod, Plan } from "@/lib/ipc/types";

/** TMP-009: the supporting documents a contract may name, a closed list without any cheque. The
 * kind select offers these and nothing else, so no screen can ask for a guarantee cheque. */
export const GUARANTEE_KINDS: readonly GuaranteeKind[] = [
  "id_document",
  "work_certificate",
  "pay_slip",
  "ccp_account",
  "residence_certificate",
  "guarantor",
  "other",
];

/** How the down payment or a due is paid (the backend refuses credit, store credit and change). */
export const PAY_METHODS: readonly PaymentMethod[] = [
  "cash",
  "card_cib",
  "edahabia",
  "baridimob",
  "cheque",
  "transfer",
];

/** The badge of a due's state: paid, partly paid or due today, late. */
export function dueTone(status: DueState["status"]): "ok" | "warn" | "danger" | "neutral" {
  switch (status) {
    case "paid":
      return "ok";
    case "partial":
    case "due":
      return "warn";
    case "overdue":
      return "danger";
    case "upcoming":
      return "neutral";
  }
}

/** The first due not fully paid (the payments fill the oldest first), if any. */
export function nextDue(plan: Plan): DueState | undefined {
  return plan.dues.find((d) => d.status !== "paid");
}

/** The longest delay of a contract's late dues, 0 when none is late. */
export function daysLate(plan: Plan): number {
  return plan.dues.reduce((most, d) => (d.status === "overdue" ? Math.max(most, d.daysLate) : most), 0);
}
