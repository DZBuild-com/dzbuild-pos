import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { toast } from "sonner";
import { isApprovalCancelled, useApproval } from "@/components/manager-approval";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { formatDZD } from "@/lib/format";
import { useHardwareConfig } from "@/lib/ipc/hardware";
import {
  isPrepaid,
  useClaimOrder,
  useHandOverOrder,
  useOnlineOrder,
  usePrepareOrder,
  useReleaseOrder,
  useShipOrder,
} from "@/lib/ipc/online-orders";
import type { Session } from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { BuyerFlag } from "./BuyerFlag";
import { RefuseDialog } from "./RefuseDialog";
import { SellOrderDialog } from "./SellOrderDialog";

/** One online order: the buyer (with DZBuild's flag, a warning only), the lines at their online
 * prices against the shop's, and the steps this till may take: claim, prepare, take payment at
 * the till, ship with a delivery note, put back, refuse. */
export function OrderDetail({ id, session }: { id: string; session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const errorText = useErrorText();
  const { data: o, error } = useOnlineOrder(id);
  const claim = useClaimOrder();
  const release = useReleaseOrder();
  const prepare = usePrepareOrder();
  const ship = useShipOrder();
  const handOver = useHandOverOrder();
  const hardware = useHardwareConfig();
  // a delivery note at the online prices may need a manager, as the sale at the till does
  const approval = useApproval();
  const [selling, setSelling] = useState(false);
  const [refusing, setRefusing] = useState(false);
  if (error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(error)}
      </p>
    );
  }
  if (!o) return null;
  const mine = o.claimedByMe && (o.localState === "claimed" || o.localState === "prepared");
  const canShip = session.permissions.includes("documents.delivery");
  // paid online: handed over, never cashed again at the till nor left to collect on a BL
  const prepaid = isPrepaid(o.paymentMethod);
  const busy =
    claim.isPending || release.isPending || prepare.isPending || ship.isPending || handOver.isPending;
  const fail = (e: unknown) => toast.error(errorText(e));
  const shipOrder = () => {
    approval
      .run(() => ship.mutateAsync({ id: o.id, label: t("sync:orders.shippingLine") }))
      .then((doc) => navigate(`/documents/${doc.id}`))
      .catch((e: unknown) => {
        if (!isApprovalCancelled(e)) fail(e);
      });
  };
  const handOverOrder = (courier: boolean) => {
    const printReceipt =
      Boolean(hardware.data?.receiptPrinterId) && (hardware.data?.receipt?.printByDefault ?? true);
    approval
      .run(() =>
        handOver.mutateAsync({ id: o.id, label: t("sync:orders.shippingLine"), courier, printReceipt }),
      )
      .then((done) => toast.success(t("sync:orders.handedOver", { number: done.sale.document.number ?? "" })))
      .catch((e: unknown) => {
        if (!isApprovalCancelled(e)) fail(e);
      });
  };
  const place = [o.commune, o.address].filter(Boolean).join(" · ");
  return (
    <Card className="min-w-0 grow gap-3">
      <h2 className="text-2xl font-bold">
        {t("sync:orders.title", { number: o.orderNumber })} · {t(`sync:orders.states.${o.localState}`)}
      </h2>
      <BuyerFlag phone={o.customerPhone} />
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
        <dt className="text-ink-2">{t("sync:orders.buyer")}</dt>
        <dd>
          {o.customerName ?? t("sync:orders.noName")}{" "}
          {o.customerPhone ? <bdi dir="ltr">{o.customerPhone}</bdi> : null}
        </dd>
        <dt className="text-ink-2">{t("sync:orders.payment")}</dt>
        <dd>
          {paymentLabel(t, o.paymentMethod)}
          {prepaid ? <span className="block text-sm text-ink-2">{t("sync:orders.prepaidNote")}</span> : null}
        </dd>
        <dt className="text-ink-2">{t("sync:orders.delivery")}</dt>
        <dd>
          {t(`sync:orders.deliveryTypes.${o.deliveryType}`)}
          {place ? ` · ${place}` : ""}
        </dd>
        {o.customerNotes ? (
          <>
            <dt className="text-ink-2">{t("sync:orders.notes")}</dt>
            <dd>{o.customerNotes}</dd>
          </>
        ) : null}
        {o.refuseReason ? (
          <>
            <dt className="text-ink-2">{t("sync:orders.refusedFor")}</dt>
            <dd>{t(`sync:orders.reasons.${o.refuseReason}`)}</dd>
          </>
        ) : null}
      </dl>
      <ul>
        {o.lines.map((l) => (
          <li key={l.lineNo} className="flex items-start gap-3 border-t border-line py-2">
            <span className="grow">
              {l.productName}
              {l.variantName ? ` · ${l.variantName}` : ""}
              {l.productId === null ? (
                <span className="block text-sm text-danger">{t("sync:orders.unmapped")}</span>
              ) : null}
              {l.localPrice !== null && l.localPrice !== l.unitPrice ? (
                <span className="block text-sm text-warn">
                  {t("sync:orders.priceDiff", {
                    online: formatDZD(l.unitPrice, lang),
                    shop: formatDZD(l.localPrice, lang),
                  })}
                </span>
              ) : null}
            </span>
            <bdi dir="ltr" className="tabular-nums">
              {formatQty(l.qty, lang)} × {formatDZD(l.unitPrice, lang)}
            </bdi>
          </li>
        ))}
      </ul>
      <p className="border-t border-line pt-2 text-end text-2xl font-bold">
        <bdi dir="ltr" className="tabular-nums">
          {formatDZD(o.total, lang)}
        </bdi>
      </p>
      {o.localDocumentNumber ? <p>{t("sync:orders.document", { number: o.localDocumentNumber })}</p> : null}
      <div className="flex flex-wrap gap-2">
        {o.localState === "new" && !o.claimedElsewhere ? (
          <Button variant="primary" disabled={busy} onClick={() => claim.mutate(o.id, { onError: fail })}>
            {t("sync:orders.claim")}
          </Button>
        ) : null}
        {mine && o.localState === "claimed" ? (
          <Button variant="outline" disabled={busy} onClick={() => prepare.mutate(o.id, { onError: fail })}>
            {t("sync:orders.prepare")}
          </Button>
        ) : null}
        {mine && prepaid ? (
          <>
            <Button variant="primary" disabled={busy} onClick={() => handOverOrder(false)}>
              {t("sync:orders.handOver")}
            </Button>
            <Button variant="outline" disabled={busy} onClick={() => handOverOrder(true)}>
              {t("sync:orders.handOverCourier")}
            </Button>
          </>
        ) : null}
        {mine ? (
          <>
            {prepaid ? null : (
              <Button variant="primary" disabled={busy} onClick={() => setSelling(true)}>
                {t("sync:orders.sell")}
              </Button>
            )}
            {canShip && !prepaid ? (
              <Button variant="outline" disabled={busy} onClick={shipOrder}>
                {t("sync:orders.ship")}
              </Button>
            ) : null}
            <Button variant="ghost" disabled={busy} onClick={() => release.mutate(o.id, { onError: fail })}>
              {t("sync:orders.release")}
            </Button>
          </>
        ) : null}
        {(o.localState === "new" && !o.claimedElsewhere) || mine ? (
          <Button variant="ghost" disabled={busy} onClick={() => setRefusing(true)}>
            {t("sync:orders.refuse")}
          </Button>
        ) : null}
      </div>
      {selling ? <SellOrderDialog order={o} onClose={() => setSelling(false)} /> : null}
      {refusing ? <RefuseDialog order={o} onClose={() => setRefusing(false)} /> : null}
      {approval.dialog}
    </Card>
  );
}

/** How the buyer pays, in the till's words: cash on delivery, paid online, free; another method
 * DZBuild may add reads as paid online with its own name. */
export function paymentLabel(
  t: (key: string, options?: Record<string, unknown>) => string,
  method: string,
): string {
  const known = ["cod", "digital_payment", "free_digital"];
  const key = method.trim().toLowerCase();
  return known.includes(key) ? t(`sync:orders.payments.${key}`) : t("sync:orders.payments.other", { method });
}
