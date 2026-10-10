import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useLinkSale } from "@/lib/ipc/billing";
import { useExchange } from "@/lib/ipc/returns";
import { useCreateSale } from "@/lib/ipc/sales";
import type { HardwareConfig, PaymentInput, SaleExtras } from "@/lib/ipc/types";
import { type ExchangeDraft, toCart, useCart } from "./cart-store";
import { useExtras } from "./extras-store";
import { type Tender, tenderTypes } from "./payment-model";
import type { Done } from "./PosDialogs";

type Run = <T>(action: () => Promise<T>) => Promise<T>;

/** Whether the receipt prints when the cashier did not choose (SAL-052): the shop's default, and
 * never without a receipt printer. */
export function receiptByDefault(hardware: HardwareConfig | undefined): boolean {
  return Boolean(hardware?.receiptPrinterId) && (hardware?.receipt?.printByDefault ?? true);
}

/** What Valider sends. */
export interface FinishInput {
  /** The tenders as typed (their custom payment types go in the extras). */
  tenders: Tender[];
  /** The same tenders as the backend reads them (`toInputs`). */
  inputs: PaymentInput[];
  printReceipt: boolean;
  /** A return paying part of the ticket: the sale goes through `returns_exchange`. */
  exchange: ExchangeDraft | null;
  /** A pack flow's own command in place of `sales_create` (a repair's pickup, a table ticket). */
  onPay?: (payments: PaymentInput[], printReceipt: boolean, extras: SaleExtras) => Promise<Done | void>;
}

/**
 * Valider: the till's cart and the tenders go to `sales_create` (or `returns_exchange`, or the
 * pack flow's `onPay`) under the manager's approval when the backend asks for one (`run`). The
 * payment dialog and the till's "one method" checkout both finish a sale through here, so the
 * extras, the payment types and the approval are the same whichever way it was paid.
 * Resolves the `Done` to show, if any; rejects with the backend's error (or the approval's
 * cancellation).
 */
export function useFinishSale(run: Run) {
  const createSale = useCreateSale();
  const createExchange = useExchange();
  // a pack flow's own command is in flight
  const [paying, setPaying] = useState(false);
  const finish = async ({
    tenders,
    inputs,
    printReceipt,
    exchange,
    onPay,
  }: FinishInput): Promise<Done | null> => {
    const cart = toCart(useCart.getState());
    // M8: the sale's pack part (serials, prescriptions, the buyer, the van) and its payment types
    const extras = { ...useExtras.getState().toExtras(), paymentTypes: tenderTypes(tenders) };
    if (onPay) {
      setPaying(true);
      const done = await run(() => onPay(inputs, printReceipt, extras)).finally(() => setPaying(false));
      return done === undefined ? null : done;
    }
    if (exchange) {
      const result = await run(() =>
        createExchange.mutateAsync({
          ret: exchange.input,
          sale: { cart, payments: inputs },
          printReceipt,
          returnExtras: exchange.extras,
          saleExtras: extras,
        }),
      );
      return { kind: "exchange", result } satisfies Done;
    }
    const result = await run(() =>
      createSale.mutateAsync({ sale: { cart, payments: inputs }, printReceipt, extras }),
    );
    return { kind: "sale", result } satisfies Done;
  };
  return { finish, busy: createSale.isPending || createExchange.isPending || paying };
}

/** A ticket was paid at the till: the cart is emptied, the ticket linked to the quote it was made
 * from, and the done screen shown (`show`). */
export function useCommitDone(show: (done: Done) => void) {
  const { t } = useTranslation();
  const linkSale = useLinkSale();
  return (done: Done) => {
    // the ticket is committed: its lines and its return leave the cart now, not on
    // "Nouvelle vente", so a lock or a logout on the change screen cannot bring them
    // back to be sold or refunded twice (the done dialog reads only `done.result`)
    const state = useCart.getState();
    const quote = state.fromQuote;
    state.clear(state.lang);
    // SAL-046: the ticket is recorded as made from the quote; the sale stands if it fails
    const ticket =
      done.kind === "sale"
        ? done.result.sale.document
        : done.kind === "exchange"
          ? done.result.exchange.sale.document
          : null;
    if (quote && ticket) {
      linkSale.mutate(
        { quoteId: quote.id, ticketId: ticket.id },
        { onError: () => toast.warning(t("billing:offer.linkFailed")) },
      );
    }
    show(done);
  };
}
