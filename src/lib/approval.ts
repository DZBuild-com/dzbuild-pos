import { toAppError } from "@/lib/ipc/client";
import type { Permission } from "@/lib/ipc/types";

/** Backend refusals a manager can lift, and the permission the manager grants (spec §4.5). */
const BY_CODE: Record<string, Permission> = {
  price_override_not_allowed: "sales.price_override",
  free_line_not_allowed: "sales.price_override",
  discount_not_allowed: "sales.discount",
  discount_above_limit: "sales.discount",
  return_without_receipt_not_allowed: "sales.price_override",
  variance_needs_approval: "cash.view_expected",
  // M5: a Z on a till with operations still to send to the main PC (offline Z)
  offline_z_needs_approval: "cash.view_expected",
  // M2: a credit sale above the customer's limit, a price below the product's floor
  credit_limit_exceeded: "credit.limit_override",
  price_below_floor: "sales.price_override",
};

export function approvalPermission(error: unknown): Permission | null {
  const e = toAppError(error);
  if (e.code === "insufficient_permission") {
    const permission = (e.details as { permission?: unknown } | null | undefined)?.permission;
    return typeof permission === "string" ? (permission as Permission) : null;
  }
  return BY_CODE[e.code] ?? null;
}

/** Thrown by `useApproval().run` when the cashier closes the approval dialog. */
export class ApprovalCancelled extends Error {
  constructor() {
    super("approval cancelled");
    this.name = "ApprovalCancelled";
  }
}

export function isApprovalCancelled(error: unknown): error is ApprovalCancelled {
  return error instanceof ApprovalCancelled;
}
