import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { isApprovalCancelled, useApproval } from "@/components/manager-approval";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { formatDZD } from "@/lib/format";
import { useHardwareConfig } from "@/lib/ipc/hardware";
import { useOrderQuote, useSellOrder } from "@/lib/ipc/online-orders";
import type { OnlineOrder, PaymentInput } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** Picked up at the shop or cash brought back by the courier: the till's sale at the online
 * prices, paid in cash (duty, rounding, change, drawer and receipt as for any sale). Every figure
 * is the backend's quote for the cash typed; nothing typed is the exact amount. */
export function SellOrderDialog({ order, onClose }: { order: OnlineOrder; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const label = t("sync:orders.shippingLine");
  const [cash, setCash] = useState("");
  const typed = parseAmount(cash);
  const tenders = useMemo<PaymentInput[]>(
    () =>
      typed === null
        ? []
        : [{ method: "cash", amount: typed, reference: null, chequeBank: null, chequeDate: null }],
    [typed],
  );
  const quote = useOrderQuote(order.id, label, tenders);
  const settlement = quote.isPlaceholderData ? undefined : quote.data?.settlement;
  const due = quote.data?.settlement.totalDue;
  const hardware = useHardwareConfig();
  // the shop's receipt default, only where a receipt printer is set (as the payment dialog does)
  const printReceipt =
    Boolean(hardware.data?.receiptPrinterId) && (hardware.data?.receipt?.printByDefault ?? true);
  const sell = useSellOrder();
  const approval = useApproval();
  const [error, setError] = useState<unknown>(null);
  const ready = settlement !== undefined && (typed === null || settlement.missing === 0);
  const submit = async () => {
    if (due === undefined) return;
    setError(null);
    const payments: PaymentInput[] = [
      { method: "cash", amount: typed ?? due, reference: null, chequeBank: null, chequeDate: null },
    ];
    try {
      const done = await approval.run(() =>
        sell.mutateAsync({ id: order.id, shippingLabel: label, payments, printReceipt }),
      );
      toast.success(t("sync:orders.sold", { number: done.sale.document.number ?? "" }));
      onClose();
    } catch (e) {
      if (!isApprovalCancelled(e)) setError(e);
    }
  };
  return (
    <>
      <Dialog
        open={!approval.pending}
        onOpenChange={(open) => {
          if (!open) onClose();
        }}
        title={t("sync:orders.sellTitle", { number: order.orderNumber })}
        footer={
          <>
            <Button variant="outline" onClick={onClose}>
              {t("common:actions.cancel")}
            </Button>
            <Button
              variant="primary"
              size="lg"
              disabled={!ready || sell.isPending}
              onClick={() => void submit()}
            >
              {due === undefined
                ? t("sync:orders.sell")
                : t("sync:orders.cashIn", { amount: formatDZD(due, lang) })}
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-1">
          <span className="text-ink-2">{t("payments:totalDue")}</span>
          <p className="text-5xl leading-tight font-bold tabular-nums">
            {due === undefined ? (
              <>
                <span aria-hidden>—</span>
                <span className="sr-only">{t("payments:unknown")}</span>
              </>
            ) : (
              <bdi dir="ltr">{formatDZD(due, lang)}</bdi>
            )}
          </p>
          {settlement && settlement.stampCharged > 0 ? (
            <span className="text-sm text-ink-2">
              {t("payments:stampDuty")} <bdi dir="ltr">{formatDZD(settlement.stampCharged, lang)}</bdi>
            </span>
          ) : null}
        </div>
        <Field id="order-cash" label={t("sync:orders.cashGiven")}>
          <NumericInput
            id="order-cash"
            value={cash}
            onValueChange={setCash}
            placeholder={due === undefined ? undefined : formatDZD(due, lang)}
          />
        </Field>
        {typed !== null && settlement && settlement.change > 0 ? (
          <p className="text-xl font-semibold">
            {t("sync:orders.change", { amount: formatDZD(settlement.change, lang) })}
          </p>
        ) : null}
        {quote.error || error ? (
          <p role="alert" className="text-danger">
            {errorText(error ?? quote.error)}
          </p>
        ) : null}
      </Dialog>
      {approval.dialog}
    </>
  );
}
