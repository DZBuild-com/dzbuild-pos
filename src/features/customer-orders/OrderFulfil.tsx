import { Suspense, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Tender } from "@/features/pos/payment-model";
import type { PosContext } from "@/features/pos/PosDialogs";
import { STEP_MODULES, type TillPieces } from "@/features/services/ServicePickup";
import { formatAmountInput, formatDZD } from "@/lib/format";
import { useCurrentSession } from "@/lib/ipc/cash";
import { useFulfilCart, useFulfilOrder, useOrder } from "@/lib/ipc/orders";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const tender = (method: Tender["method"], amount = "", reference = ""): Tender => ({
  method,
  amount,
  reference,
  chequeBank: "",
  chequeDate: "",
});

/**
 * "Servir en caisse" (ORD-002/003): the order's lines come into the (empty) till at the ordered
 * prices with its customer, the payment dialog opens with the deposit held as a `voucher` tender
 * naming `CC…` and the rest to pay (a layaway's rest is paid now: never on credit), and Valider
 * sells it through `orders_fulfil` (the deposit spent, the order fulfilled, the receipt printed).
 * Cancelling leaves the till empty again.
 *
 * Mounted lazily by the till's dialogs (`PosDialogs`, kind `fulfil`), never in the till's entry.
 */
export function OrderFulfil({
  orderId,
  ctx,
  till: { PaymentDialog, useCart, PackDialog },
  onClose,
}: {
  orderId: string;
  ctx: PosContext;
  till: TillPieces;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const order = useOrder(orderId);
  const cart = useFulfilCart(orderId);
  const drawer = useCurrentSession();
  const fulfil = useFulfilOrder();
  const lines = useCart((s) => s.lines);
  const note = useCart((s) => s.note);
  const data = order.data;
  const loaded =
    data !== undefined && note === data.number && lines.length === (cart.data?.lines.length ?? -1);
  // final review: the serials, register entries and buyer the sale needs, as "Encaisser" asks
  const steps = ctx.modules.some((m) => (STEP_MODULES as readonly string[]).includes(m));
  const [checked, setChecked] = useState(false);
  const checkoutCtx = useMemo(() => ({ ...ctx, checkoutDone: () => setChecked(true) }), [ctx]);

  // Once the order, its cart and the drawer are known: refuse what cannot be served here, else load
  // the till's cart (the cart store, not this component, holds it).
  useEffect(() => {
    const error = order.error ?? cart.error;
    if (error) {
      ctx.notify("error", errorText(error));
      onClose();
      return;
    }
    if (!data || !cart.data || !drawer.isSuccess || loaded) return;
    const till = useCart.getState();
    const problem =
      drawer.data === null
        ? t("pos:closed.body")
        : data.status !== "open" && data.status !== "ready"
          ? t("errors:order_status_invalid")
          : till.lines.length > 0 || till.exchange
            ? t("pos:held.busy")
            : null;
    if (problem) {
      ctx.notify("error", problem);
      onClose();
      return;
    }
    till.load(
      { ...cart.data, note: data.number },
      data.lines.map((line) => ({
        name: line.description,
        unitCode: "pc",
        unitDecimals: line.qty % 1_000 === 0 ? 0 : 3,
        price: line.unitPrice,
      })),
    );
    ctx.notify(
      "info",
      data.kind === "layaway"
        ? t("orders:fulfil.layaway", { number: data.number })
        : t("orders:fulfil.loaded", { number: data.number, held: formatDZD(data.held, lang) }),
    );
  }, [
    cart.data,
    cart.error,
    ctx,
    data,
    drawer.data,
    drawer.isSuccess,
    errorText,
    lang,
    loaded,
    onClose,
    order.error,
    t,
    useCart,
  ]);

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
  const deposit = data.held > 0 ? [tender("voucher", formatAmountInput(data.held, lang), data.number)] : [];
  return (
    <PaymentDialog
      initialTenders={[...deposit, tender("cash")]}
      onClose={close}
      onPay={async (payments, printReceipt, extras) => {
        const done = await fulfil.mutateAsync({ input: { id: data.id, payments, extras }, printReceipt });
        return { kind: "sale", result: { sale: done.fulfilled.sale, printQueued: done.printQueued } };
      }}
      onDone={(done) => {
        // the order is served: the done screen first, then the till is empty again
        ctx.open({ kind: "done", done });
        useCart.getState().clear(useCart.getState().lang);
      }}
    />
  );
}
