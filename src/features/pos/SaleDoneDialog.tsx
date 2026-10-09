import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { useSession } from "@/lib/ipc/auth";
import { useCustomer } from "@/lib/ipc/customers";
import type { Done } from "./PosDialogs";
import { usePaidFrame } from "./use-customer-display";

/** After Valider: the change first, big (the cashier hands it over while the receipt prints); after
 * a refund, the amount to pay back, or put on the customer's account. The customer and his balance
 * after the ticket, as the receipt prints them (SAL-037). A ticket paid shows its thanks, the
 * amount paid and the change on the customer display (SAL-047). */
export function SaleDoneDialog({ done, onNewSale }: { done: Done; onNewSale: () => void }) {
  const { t } = useTranslation();
  const seesCredit = useSession().data?.permissions.includes("credit.view") ?? false;
  const ticket =
    done.kind === "sale"
      ? done.result.sale.document
      : done.kind === "exchange"
        ? done.result.exchange.sale.document
        : done.result.returned.document;
  const settlement =
    done.kind === "sale"
      ? done.result.sale.settlement
      : done.kind === "exchange"
        ? done.result.exchange.sale.settlement
        : null;
  usePaidFrame(
    settlement
      ? {
          lang: ticket.lang,
          total: settlement.totalDue,
          paid: settlement.paidTotal,
          change: settlement.change,
        }
      : null,
  );
  const change = settlement?.change ?? 0;
  const net = done.kind === "exchange" ? done.result.exchange.netToPay : null;
  const refund = done.kind === "return" ? done.result.returned.refund : null;
  // RET-007: refunded to the account, nothing leaves the drawer
  const toAccount =
    done.kind === "return" &&
    done.result.returned.document.payments.some((p) => p.method === "credit" || p.method === "voucher");
  const balance =
    done.kind === "sale"
      ? done.result.sale.customerBalance
      : done.kind === "exchange"
        ? done.result.exchange.sale.customerBalance
        : null;
  const customer = useCustomer(ticket.customerId);
  const queued = done.result.printQueued;
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onNewSale())}
      title={t(done.kind === "return" ? "pos:returns.done" : "payments:done.title")}
      footer={
        <Button
          variant="primary"
          size="lg"
          autoFocus
          className="min-h-16 min-w-64 text-xl"
          onClick={onNewSale}
          shortcut="Enter"
          shortcutLabel={t("common:keys.enter")}
        >
          {t("payments:done.newSale")}
        </Button>
      }
    >
      {refund !== null ? (
        <div
          role="status"
          aria-label={t(toAccount ? "pos:returns.toAccount" : "pos:returns.toRefund")}
          className="flex items-baseline justify-between rounded-control bg-info-soft p-4 text-info"
        >
          <span className="text-xl font-semibold">
            {t(toAccount ? "pos:returns.toAccount" : "pos:returns.toRefund")}
          </span>
          <Amount value={refund} className="text-5xl font-bold" />
        </div>
      ) : (
        <div
          role="status"
          aria-label={t("payments:change")}
          className="flex items-baseline justify-between rounded-control bg-ok-soft p-4 text-ok"
        >
          <span className="text-xl font-semibold">{t("payments:change")}</span>
          <Amount value={change} className="text-5xl font-bold" />
        </div>
      )}
      {net !== null && net < 0 ? (
        <p className="flex justify-between text-lg">
          <span>{t("payments:done.refunded")}</span>
          <Amount value={-net} className="font-bold" />
        </p>
      ) : null}
      <p className="flex justify-between text-lg">
        <span>{t("payments:done.number")}</span>
        <bdi dir="ltr" className="font-bold">
          {ticket.number}
        </bdi>
      </p>
      {ticket.customerId !== null ? (
        <div className="flex flex-col gap-1 rounded-control border border-line p-3">
          <p className="flex justify-between gap-3">
            <span>{t("pos:customer.label")}</span>
            <span className="font-bold">{customer.data?.name ?? ""}</span>
          </p>
          {seesCredit && balance != null ? (
            <p className="flex justify-between gap-3">
              <span>{t("pos:customer.balanceAfter")}</span>
              <Amount value={balance} className="font-bold" />
            </p>
          ) : null}
        </div>
      ) : null}
      <p className="text-ink-2">{queued ? t("payments:done.printing") : t("payments:done.noReceipt")}</p>
    </Dialog>
  );
}
