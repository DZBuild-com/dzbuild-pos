import { useMemo } from "react";
import { useTillPaymentTypes } from "@/lib/ipc/completion";
import { useSetting } from "@/lib/ipc/settings";
import type { PaymentMethod, PaymentType } from "@/lib/ipc/types";
import { TENDER_METHODS } from "./payment-model";

/** The methods the till may finish a sale in at once: no details to type, no customer needed
 * (pos-core `settings::fixed_method_ok`). */
export const FIXED_METHODS = [
  "cash",
  "card_cib",
  "edahabia",
  "baridimob",
] as const satisfies readonly PaymentMethod[];

/** `payments.fixed_method` names a custom payment type as `type:<id>`. */
export const TYPE_PREFIX = "type:";

/** A tender the till starts from: a base method, or a custom type riding it. */
export interface TillTender {
  method: PaymentMethod;
  type: PaymentType | null;
}

/** How the till takes payment (Settings › Payments). */
export interface TillPayment {
  /** "One method": the tender the sale is finished in at once, without the payment dialog.
   * `null` when the till asks (or the custom type chosen is no longer live). */
  fixed: TillTender | null;
  /** The tiles the payment dialog shows, in the dialog's order; custom types follow their base
   * method. */
  allowed: readonly PaymentMethod[];
  /** The dialog opens on its first usable method with the exact amount typed, so Enter
   * validates at once: in the "one method" mode, and when the shop narrowed the list. Every
   * method allowed is today's dialog, an empty cash tender. */
  prefill: boolean;
  /** The settings have answered (or failed: then the defaults apply). */
  known: boolean;
}

/** A custom payment type the till can finish a sale in at once: it rides a method of
 * {@link FIXED_METHODS} and asks no reference. */
export function quickType(type: PaymentType): boolean {
  return !type.archived && !type.requiresReference && FIXED_METHODS.some((m) => m === type.baseMethod);
}

/** The stored list as tiles, in the dialog's order; anything unreadable is every method. */
export function allowedOf(value: unknown): PaymentMethod[] {
  if (!Array.isArray(value)) return TENDER_METHODS;
  const allowed = TENDER_METHODS.filter((m) => value.includes(m));
  return allowed.length > 0 ? allowed : TENDER_METHODS;
}

/** The stored one method as the tender it pays with; `null` for a custom type that is gone,
 * archived or no longer quick, or while `types` are not known. */
export function fixedOf(value: unknown, types: readonly PaymentType[] | undefined): TillTender | null {
  if (typeof value !== "string") return { method: "cash", type: null };
  const method = FIXED_METHODS.find((m) => m === value);
  if (method) return { method, type: null };
  if (!value.startsWith(TYPE_PREFIX)) return { method: "cash", type: null };
  const id = value.slice(TYPE_PREFIX.length);
  const type = types?.find((t) => t.id === id);
  return type && quickType(type) ? { method: type.baseMethod, type } : null;
}

/** The three settings read together; unread or unset values are the backend's defaults. */
export function tillPayment(
  mode: unknown,
  fixed: unknown,
  allowed: unknown,
  types: readonly PaymentType[] | undefined,
): TillPayment {
  if (mode === "fixed") {
    const tender = fixedOf(fixed, types);
    return { fixed: tender, allowed: TENDER_METHODS, prefill: tender !== null, known: true };
  }
  const list = allowedOf(allowed);
  return { fixed: null, allowed: list, prefill: list.length < TENDER_METHODS.length, known: true };
}

/** How this till takes payment. The settings replicate from the main PC like the others. */
export function useTillPayment(enabled = true): TillPayment {
  const queries = [
    useSetting("payments.till_mode", enabled),
    useSetting("payments.fixed_method", enabled),
    useSetting("payments.allowed_methods", enabled),
  ] as const;
  const [mode, fixed, allowed] = queries.map((q) => q.data);
  const wantsType = mode === "fixed" && typeof fixed === "string" && fixed.startsWith(TYPE_PREFIX);
  const types = useTillPaymentTypes(enabled && wantsType);
  const known = !enabled || queries.every((q) => !q.isPending);
  const typesRead = wantsType ? types : undefined;
  return useMemo(
    () => ({ ...tillPayment(mode, fixed, allowed, typesRead), known }),
    [mode, fixed, allowed, typesRead, known],
  );
}
