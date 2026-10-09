import { ClipboardList } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Button } from "@/components/ui/button";
import { useConvert, useDraftCart } from "@/lib/ipc/billing";
import type { DocState, Document } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { OrderDialog } from "./OrderDialog";

/**
 * ORD-001 "Transformer en commande" on a validated quote or proforma: the backend makes an order
 * draft at the offered prices (at today's once the offer expired), then `OrderDialog` records it
 * from that draft with its kind, due date, reservation and deposit; the new order opens. Closed
 * without recording, the same draft opens again on the next press (no second draft).
 */
export function QuoteToOrder({ doc, state }: { doc: Document; state: DocState }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const navigate = useNavigate();
  const convert = useConvert();
  const [draft, setDraft] = useState<Document | null>(null);
  const [showing, setShowing] = useState(false);
  const cart = useDraftCart(draft?.id ?? null);
  const converted = state.state === "converted";
  return (
    <div className="flex flex-col gap-1">
      <Button
        variant="secondary"
        disabled={(converted && !draft) || convert.isPending}
        onClick={() => {
          if (draft) {
            setShowing(true);
            return;
          }
          convert.mutate(
            { sourceId: doc.id, target: "customer_order", reprice: state.state === "expired" },
            {
              onSuccess: (made) => {
                setDraft(made);
                setShowing(true);
              },
            },
          );
        }}
      >
        <ClipboardList aria-hidden />
        {t("orders:fromQuote")}
      </Button>
      {convert.error ? (
        <p role="alert" className="text-sm text-danger">
          {errorText(convert.error)}
        </p>
      ) : null}
      {showing && draft && cart.data ? (
        <OrderDialog
          cart={cart.data.cart}
          total={draft.totalTtc}
          draftId={draft.id}
          onCreated={(order) => void navigate(`/customer-orders/${order.id}`)}
          onClose={() => setShowing(false)}
        />
      ) : null}
      {cart.error ? (
        <p role="alert" className="text-sm text-danger">
          {errorText(cart.error)}
        </p>
      ) : null}
    </div>
  );
}
