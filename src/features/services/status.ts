import type { ServiceKind, ServiceStatus } from "@/lib/ipc/types";

export const SERVICE_KINDS: readonly ServiceKind[] = ["repair", "pressing", "tailoring", "other"];

/** The board's columns: a ticket still in the shop, in the order it moves. */
export const OPEN_STATUSES: readonly ServiceStatus[] = [
  "received",
  "diagnosing",
  "waiting_parts",
  "in_progress",
  "ready",
];

/** A TypeScript copy of pos-core `service_tickets::TRANSITIONS` (Task 9): the buttons a ticket
 * shows. `delivered` is reached only through the pickup at the till; the backend refuses anything
 * else with `service_status_invalid`. */
const TRANSITIONS: Readonly<Partial<Record<ServiceStatus, readonly ServiceStatus[]>>> = {
  received: ["diagnosing", "in_progress", "waiting_parts", "ready", "returned_unrepaired", "cancelled"],
  diagnosing: ["waiting_parts", "in_progress", "ready", "returned_unrepaired", "cancelled"],
  waiting_parts: ["in_progress", "ready", "returned_unrepaired", "cancelled"],
  in_progress: ["waiting_parts", "ready", "returned_unrepaired", "cancelled"],
  ready: ["in_progress", "returned_unrepaired", "cancelled"],
};

export function nextStatuses(status: ServiceStatus): readonly ServiceStatus[] {
  return TRANSITIONS[status] ?? [];
}

/** Delivered, returned unrepaired or cancelled: final (a trigger of migration 0008). */
export function isClosed(status: ServiceStatus): boolean {
  return !OPEN_STATUSES.includes(status);
}

/** The Luhn check of a 15-digit IMEI (pos-core `stock::serials::imei_ok`), for instant feedback:
 * the backend checks again. Any other serial number is free text. */
export function imeiOk(serial: string): boolean {
  if (!/^\d{15}$/.test(serial)) return false;
  let sum = 0;
  for (let i = 0; i < 15; i += 1) {
    const digit = Number(serial[14 - i]);
    const doubled = i % 2 === 1 ? digit * 2 : digit;
    sum += doubled > 9 ? doubled - 9 : doubled;
  }
  return sum % 10 === 0;
}

/** A 15-digit number that fails its IMEI check (other serial numbers are not checked). */
export function badImei(serial: string): boolean {
  return /^\d{15}$/.test(serial) && !imeiOk(serial);
}
