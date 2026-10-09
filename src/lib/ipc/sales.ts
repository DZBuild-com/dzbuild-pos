import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { Cart, CartEventInput, PaymentInput, PreviewInput, SaleRequest } from "./types";

/** Live totals for the cart and the tenders being typed; idle for `null` or an empty cart. */
export function useSalePreview(input: PreviewInput | null) {
  const idle = input === null || input.cart.lines.length === 0;
  return useQuery({
    queryKey: qk.salePreview(input),
    queryFn: () => commands.salesPreview(input as PreviewInput),
    enabled: !idle,
    // the last figures stay up while the next keystroke is priced; an emptied cart (a committed
    // or cancelled ticket) shows none, not the last ticket's total
    placeholderData: idle ? undefined : keepPreviousData,
    staleTime: 0,
  });
}

/** The payment dialog's figures for table ticket `ticketId` (M8 restaurant): the bill the payment
 * takes, its cover charge and offered lines included, and the tenders being typed; idle for `null`.
 * Every write on the ticket refreshes it (`qk.dining.all`). */
export function useTicketPreview(ticketId: string | null, tenders: PaymentInput[]) {
  return useQuery({
    queryKey: qk.dining.preview(ticketId ?? "", tenders),
    queryFn: () => commands.diningTicketPreview(ticketId ?? "", tenders),
    enabled: ticketId !== null,
    placeholderData: keepPreviousData,
    staleTime: 0,
  });
}

/** VRT-010: what 1 kg of `productId` costs this cart's customer (his price level, his discount,
 * today's promotion), TTC as the till will charge it: the backend prices a 1 kg line. */
export function useKiloPrice(
  productId: string | null,
  context: Pick<Cart, "customerId" | "priceLevelId" | "lang">,
) {
  return useQuery({
    queryKey: ["sales", "kiloPrice", productId, context] as const,
    queryFn: () =>
      commands.salesPreview({
        cart: {
          ...context,
          note: null,
          lines: [
            {
              productId,
              description: null,
              qty: 1_000,
              unitPrice: null,
              discount: { kind: "none" },
              taxRateId: null,
              note: null,
              expectedUnitPrice: null,
            },
          ],
        },
        tenders: [],
        storeCredit: null,
      }),
    enabled: productId !== null,
    select: (preview) => preview.cart.lines[0]?.unitPriceTtc ?? null,
  });
}

/** A cart about to be loaded at the till (a quote sold at the till), priced once by the backend:
 * the names, units and prices its lines show, and the lines whose price changed. */
export function usePriceCart() {
  return useMutation({
    mutationFn: (cart: Cart) => commands.salesPreview({ cart, tenders: [], storeCredit: null }),
  });
}

/** A ticket moves the drawer, the history, the stock and, on credit or store credit, the
 * customer's account (his balance at the till, his file, his statement). */
function useRefreshAfterSale() {
  const client = useQueryClient();
  return () =>
    Promise.all(
      [
        ["cash"],
        ["documents"],
        ["stock"],
        ["products", "grid"],
        ["sales", "held"],
        ["customers"],
        ["credit"],
      ].map((queryKey) => client.invalidateQueries({ queryKey })),
    );
}

export function useCreateSale() {
  const refresh = useRefreshAfterSale();
  return useMutation({
    mutationFn: (request: SaleRequest) => commands.salesCreate(request),
    onSuccess: refresh,
  });
}

export function useHeldCarts(enabled = true) {
  return useQuery({ queryKey: qk.held, queryFn: () => commands.salesHeld(), enabled });
}

function useRefreshHeld() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: qk.held });
}

export function useHoldCart() {
  const refresh = useRefreshHeld();
  return useMutation({
    mutationFn: ({ label, cart }: { label: string; cart: Cart }) => commands.salesHold(label, cart),
    onSuccess: refresh,
  });
}

export function useRecallCart() {
  const refresh = useRefreshHeld();
  return useMutation({ mutationFn: (id: string) => commands.salesRecall(id), onSuccess: refresh });
}

export function useDiscardHeld() {
  const refresh = useRefreshHeld();
  return useMutation({ mutationFn: (id: string) => commands.salesDiscardHeld(id), onSuccess: refresh });
}

/** Journals a void or a cancelled cart: the till sends the cart, the backend values it (AUD-001). */
export function useRecordCartEvent() {
  return useMutation({ mutationFn: (event: CartEventInput) => commands.salesRecordEvent(event) });
}
