import type { QueryClient } from "@tanstack/react-query";
import { commands } from "@/lib/ipc/client";
import { qk } from "@/lib/ipc/keys";
import { jewelPiece } from "@/lib/ipc/till-packs";
import type { Module } from "@/lib/ipc/types";
import type { CheckoutLine } from "./checkout-steps";

/** What the checkout steps read about the products (M8): cached a minute, a fact that cannot be
 * read asks nothing (the backend checks the sale again at "Valider"). Shared by the till's
 * "Encaisser" and by the sales other screens make (an instalment contract, a served order, a
 * repair handed back). */
const CACHE_MS = 60_000;

/** SAL-025: the product is sold by serial (its extras' `trackSerials`). */
export async function tracksSerials(client: QueryClient, productId: string): Promise<boolean> {
  const extras = await client.fetchQuery({
    queryKey: qk.productExtras(productId),
    queryFn: () => commands.productsExtrasGet(productId),
    staleTime: CACHE_MS,
  });
  return extras.trackSerials;
}

export async function controlled(client: QueryClient, productId: string): Promise<boolean> {
  const row = await client.fetchQuery({
    queryKey: qk.pharmacy.product(productId),
    queryFn: () => commands.pharmacyProductGet(productId),
    staleTime: CACHE_MS,
  });
  return row?.controlled ?? false;
}

export async function jewel(client: QueryClient, productId: string): Promise<boolean> {
  const piece = await client.fetchQuery({
    queryKey: ["till", "jewel", productId],
    queryFn: () => jewelPiece(productId),
    staleTime: CACHE_MS,
  });
  return piece !== null;
}

/** The jeweller register takes the buyer from the customer's file when it holds a NIN and an
 * address (core `jewellery::buyer_of`). */
export async function identified(client: QueryClient, customerId: string): Promise<boolean> {
  const customer = await client.fetchQuery({
    queryKey: qk.customer(customerId),
    queryFn: () => commands.customersGet(customerId),
    staleTime: CACHE_MS,
  });
  return Boolean(customer.nin?.trim() && customer.address?.trim());
}

/** One line of a sale as the steps read it: its product, its name, its quantity (milli-units)
 * and the serials it already has. */
export interface FactLine {
  productId: string | null;
  name: string;
  qty: number;
  serials: number;
}

/** The facts of `line` with the `modules` on. */
export function lineFacts(
  client: QueryClient,
  modules: readonly Module[],
  line: FactLine,
): Promise<CheckoutLine> {
  const ask = (module: Module, read: () => Promise<boolean>) =>
    modules.includes(module) ? read().catch(() => false) : Promise.resolve(false);
  const base = {
    productId: line.productId,
    name: line.name,
    pieces: Math.floor(line.qty / 1_000),
    serials: line.serials,
  };
  const productId = line.productId;
  if (!productId) return Promise.resolve({ ...base, trackSerials: false, controlled: false, jewel: false });
  return Promise.all([
    ask("serials", () => tracksSerials(client, productId)),
    ask("pharmacy", () => controlled(client, productId)),
    ask("jewellery", () => jewel(client, productId)),
  ]).then(([trackSerials, isControlled, isJewel]) => ({
    ...base,
    trackSerials,
    controlled: isControlled,
    jewel: isJewel,
  }));
}

/** Whether the customer's file identifies the buyer of a jewel (with the jewellery module on). */
export function customerIdentified(
  client: QueryClient,
  modules: readonly Module[],
  customerId: string | null,
): Promise<boolean> {
  if (!customerId || !modules.includes("jewellery")) return Promise.resolve(false);
  return identified(client, customerId).catch(() => false);
}
