import type { TFunction } from "i18next";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Sheet } from "@/components/ui/sheet";
import { isApprovalCancelled } from "@/lib/approval";
import { formatDate } from "@/lib/format";
import { useDiscardHeld, useHeldCarts } from "@/lib/ipc/sales";
import type { RepricedCart } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { useCart } from "./cart-store";
import type { PosContext } from "./PosDialogs";

/** A recalled ticket back in the cart, with a notice when prices changed meanwhile (spec §7.4). */
export function loadRecalled(recalled: RepricedCart, notify: PosContext["notify"], t: TFunction): void {
  const displays = recalled.priced.lines.map((l) => ({
    name: l.description,
    unitCode: l.unitCode,
    unitDecimals: l.unitCode === "pc" ? 0 : 3,
    price: l.unitPrice,
  }));
  useCart.getState().load(recalled.cart, displays);
  if (recalled.changedLines.length > 0) {
    notify("info", t("pos:held.repriced", { count: recalled.changedLines.length }));
  }
}

/** F7: the recall runs through the sale screen (`ctx.recallHeld`), which outlives this sheet and
 * closes it once the ticket is back. */
export function HeldTicketsSheet({ ctx, onClose }: { ctx: PosContext; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const held = useHeldCarts();
  const discard = useDiscardHeld();
  const busy = useCart((s) => s.lines.length > 0);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [recalling, setRecalling] = useState(false);
  const [recallError, setRecallError] = useState<unknown>(null);
  const [discardError, setDiscardError] = useState<unknown>(null);
  const recall = (id: string) => {
    setRecalling(true);
    setRecallError(null);
    ctx.recallHeld(id).then(
      () => setRecalling(false),
      (error: unknown) => {
        setRecalling(false);
        setRecallError(error);
      },
    );
  };
  return (
    <Sheet open onOpenChange={(open) => (open ? undefined : onClose())} title={t("pos:held.title")}>
      {busy ? <p className="mb-3 rounded-control bg-warn-soft p-3 text-warn">{t("pos:held.busy")}</p> : null}
      {(held.data ?? []).length === 0 ? <p className="text-ink-2">{t("pos:held.empty")}</p> : null}
      <ul className="flex flex-col gap-2">
        {(held.data ?? []).map((h) => (
          <li key={h.id} className="flex flex-col gap-2 rounded-card border border-line p-3">
            <div className="flex items-baseline justify-between">
              <span className="text-lg font-bold">{h.label}</span>
              <bdi dir="ltr" className="text-sm text-ink-2">
                {formatDate(h.createdAt, lang, { withTime: true })}
              </bdi>
            </div>
            <span className="text-ink-2">{t("pos:held.lines", { count: h.cart.lines.length })}</span>
            <div className="flex gap-2">
              <Button
                variant="primary"
                aria-label={t("pos:held.recallNamed", { label: h.label })}
                disabled={busy || recalling}
                onClick={() => recall(h.id)}
              >
                {t("pos:held.recall")}
              </Button>
              {confirm === h.id ? (
                <Button
                  variant="danger"
                  disabled={discard.isPending}
                  onClick={() => {
                    setDiscardError(null);
                    // Throwing a held ticket away cancels a ticket: sales.void_line, else a manager.
                    ctx
                      .run(() => discard.mutateAsync(h.id))
                      .then(
                        () => setConfirm(null),
                        (error: unknown) => {
                          if (!isApprovalCancelled(error)) setDiscardError(error);
                        },
                      );
                  }}
                >
                  {t("pos:held.confirmDiscard")}
                </Button>
              ) : (
                <Button variant="outline" onClick={() => setConfirm(h.id)}>
                  {t("pos:held.discard")}
                </Button>
              )}
            </div>
          </li>
        ))}
      </ul>
      {recallError || discardError ? (
        <p role="alert" className="text-danger">
          {errorText(recallError ?? discardError)}
        </p>
      ) : null}
    </Sheet>
  );
}
