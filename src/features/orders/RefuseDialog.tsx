import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useRefuseOrder } from "@/lib/ipc/online-orders";
import type { OnlineOrder, RefuseReason } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";

const REASONS: readonly RefuseReason[] = ["out_of_stock", "customer_unreachable", "duplicate", "other"];

/** The order is cancelled on DZBuild with the reason (sent when DZBuild is reachable). */
export function RefuseDialog({ order, onClose }: { order: OnlineOrder; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const refuse = useRefuseOrder();
  const [reason, setReason] = useState<RefuseReason>("out_of_stock");
  const [note, setNote] = useState("");
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t("sync:orders.refuseTitle", { number: order.orderNumber })}
      description={t("sync:orders.refuseBody")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="danger"
            disabled={refuse.isPending}
            onClick={() =>
              refuse.mutate({ id: order.id, reason, note: note.trim() || null }, { onSuccess: onClose })
            }
          >
            {t("sync:orders.refuse")}
          </Button>
        </>
      }
    >
      <fieldset aria-label={t("sync:orders.refusedFor")} className="flex flex-col gap-1">
        {REASONS.map((r) => (
          <label key={r} className="flex min-h-11 items-center gap-3">
            <input
              type="radio"
              name="refuse-reason"
              className="size-5"
              checked={reason === r}
              onChange={() => setReason(r)}
            />
            {t(`sync:orders.reasons.${r}`)}
          </label>
        ))}
      </fieldset>
      <Field id="refuse-note" label={t("sync:orders.refuseNote")}>
        <Input id="refuse-note" value={note} maxLength={200} onChange={(e) => setNote(e.target.value)} />
      </Field>
      {refuse.error ? (
        <p role="alert" className="text-danger">
          {errorText(refuse.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
