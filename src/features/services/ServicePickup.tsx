import { Suspense, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { useCart } from "@/features/pos/cart-store";
import type { Tender } from "@/features/pos/payment-model";
import type { PackDialog } from "@/features/pos/pack-slots";
import type { PaymentDialog } from "@/features/pos/PaymentDialog";
import type { PosContext } from "@/features/pos/PosDialogs";
import { formatAmountInput } from "@/lib/format";
import { useCurrentSession } from "@/lib/ipc/cash";
import { usePickup, useServiceTicket } from "@/lib/ipc/services";
import type { Cart, SaleExtras, ServiceTicket } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** The ticket's parts and labour as the till's cart, its customer and language; the note names the
 * ticket, as on the pickup's own ticket (`service_tickets::pickup`). */
function cartOf(ticket: ServiceTicket): Cart {
  return {
    lines: ticket.items.map((item) => ({
      productId: item.productId,
      description: null,
      qty: item.qty,
      unitPrice: item.unitPrice,
      discount: { kind: "none" },
      taxRateId: null,
      note: null,
      expectedUnitPrice: null,
    })),
    priceLevelId: null,
    customerId: ticket.customerId,
    lang: ticket.lang,
    note: ticket.number,
  };
}

/** The till's own pieces, handed over by `PosDialogs`: this lazy chunk imports none of the till's
 * modules, so the till's entry is not split around them (NFR-010). */
export interface TillPieces {
  PaymentDialog: typeof PaymentDialog;
  useCart: typeof useCart;
  /** The till's pack dialogs: "Encaisser"'s steps (serials, register entries, the buyer) run
   * before the payment dialog of a served order or a pickup (final review). */
  PackDialog: typeof PackDialog;
}

/** The payment types of `extras` once the payment at index `dropped` leaves the payments sent:
 * the later ones move down one, the dropped one's type goes with it. */
export function withoutPayment(extras: SaleExtras, dropped: number): SaleExtras {
  return {
    ...extras,
    paymentTypes: (extras.paymentTypes ?? []).flatMap((type) =>
      type.payment === dropped
        ? []
        : [{ ...type, payment: type.payment > dropped ? type.payment - 1 : type.payment }],
    ),
  };
}

/** The pack modules whose steps "Encaisser" runs before the payment. */
export const STEP_MODULES = ["serials", "pharmacy", "jewellery"] as const;

const tender = (method: Tender["method"], amount = "", reference = ""): Tender => ({
  method,
  amount,
  reference,
  chequeBank: "",
  chequeDate: "",
});

/** The deposit held, written as its tender reads (`voucher` naming the ticket). */
const isDeposit = (reference: string | null, ticket: ServiceTicket) =>
  reference?.trim().toUpperCase() === ticket.number;

/**
 * "Remettre au client" at the till (ORD-005): the ready ticket's lines come into the (empty) cart
 * with its customer, the payment dialog opens with the deposit held as a `voucher` tender `RP…`
 * and the rest to pay, and Valider sells it through `service_pickup` (the backend applies the
 * deposit itself, first, and delivers the ticket). Cancelling leaves the till empty again.
 *
 * Mounted lazily by the till's dialogs (`PosDialogs`, kind `pickup`), never in the till's entry.
 */
export function ServicePickup({
  ticketId,
  ctx,
  till: { PaymentDialog, useCart, PackDialog },
  onClose,
}: {
  ticketId: string;
  ctx: PosContext;
  till: TillPieces;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const ticket = useServiceTicket(ticketId);
  const drawer = useCurrentSession();
  const pickup = usePickup();
  const lines = useCart((s) => s.lines);
  const note = useCart((s) => s.note);
  const data = ticket.data;
  const loaded = data !== undefined && note === data.number && lines.length === data.items.length;
  // final review: the serials, register entries and buyer the sale needs, as "Encaisser" asks
  const steps = ctx.modules.some((m) => (STEP_MODULES as readonly string[]).includes(m));
  const [checked, setChecked] = useState(false);
  const checkoutCtx = useMemo(() => ({ ...ctx, checkoutDone: () => setChecked(true) }), [ctx]);

  // Once the ticket and the drawer are known: refuse what cannot be handed back here, else load
  // the cart (the cart store, not this component, holds it).
  useEffect(() => {
    if (ticket.error) {
      ctx.notify("error", errorText(ticket.error));
      onClose();
      return;
    }
    if (!data || !drawer.isSuccess || loaded) return;
    const cart = useCart.getState();
    const problem =
      drawer.data === null
        ? t("pos:closed.body")
        : data.status !== "ready"
          ? t("errors:service_not_ready")
          : data.items.length === 0
            ? t("services:items.empty")
            : cart.lines.length > 0 || cart.exchange
              ? t("pos:held.busy")
              : null;
    if (problem) {
      ctx.notify("error", problem);
      onClose();
      return;
    }
    cart.load(
      cartOf(data),
      data.items.map((item) => ({
        name: item.name,
        unitCode: "pc",
        unitDecimals: item.qty % 1_000 === 0 ? 0 : 3,
        price: item.unitPrice ?? 0,
      })),
    );
    ctx.notify("info", t("services:pickupLoaded", { number: data.number }));
  }, [ctx, data, drawer.data, drawer.isSuccess, errorText, loaded, onClose, t, ticket.error, useCart]);

  if (!data || !loaded) return null;
  const close = () => {
    onClose();
    useCart.getState().clear(useCart.getState().lang);
  };
  if (steps && !checked) {
    return (
      <Suspense fallback={null}>
        <PackDialog view={{ kind: "checkout" }} ctx={checkoutCtx} onClose={close} />
      </Suspense>
    );
  }
  const deposit =
    data.deposits > 0 ? [tender("voucher", formatAmountInput(data.deposits, lang), data.number)] : [];
  return (
    <PaymentDialog
      initialTenders={[...deposit, tender("cash")]}
      onClose={close}
      onPay={async (payments, printReceipt, extras) => {
        // the backend applies the whole deposit itself, before these (its payment types follow)
        const held = payments.findIndex((p) => p.method === "voucher" && isDeposit(p.reference, data));
        const done = await pickup.mutateAsync({
          input: {
            id: data.id,
            payments: payments.filter((_, index) => index !== held),
            extras: held < 0 ? extras : withoutPayment(extras, held),
          },
          printReceipt,
        });
        return { kind: "sale", result: { sale: done.picked.sale, printQueued: done.printQueued } };
      }}
      onDone={(done) => {
        // the ticket is sold: the done screen first, then the till is empty again
        ctx.open({ kind: "done", done });
        useCart.getState().clear(useCart.getState().lang);
      }}
    />
  );
}
