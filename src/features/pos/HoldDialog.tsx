import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formatDate } from "@/lib/format";
import { useHoldCart } from "@/lib/ipc/sales";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { toCart, useCart } from "./cart-store";
import type { PosContext } from "./PosDialogs";

/** F6 (SAL-003, spec §7.4): the ticket waits under a label; nothing touches stock or cash. The
 * toast offers to undo (spec §10.4) through the sale screen, which outlives this dialog. */
export function HoldDialog({ ctx, onClose }: { ctx: PosContext; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const hold = useHoldCart();
  const [label, setLabel] = useState(() =>
    t("pos:hold.default", { time: formatDate(Date.now(), lang, { withTime: true }).slice(-5) }),
  );
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const state = useCart.getState();
    hold.mutate(
      { label: label.trim() || t("pos:hold.default", { time: "" }).trim(), cart: toCart(state) },
      {
        onSuccess: (held) => {
          state.clear(state.lang);
          onClose();
          toast(t("pos:hold.done", { label: held.label }), {
            action: {
              label: t("pos:hold.undo"),
              onClick: () => {
                ctx.recallHeld(held.id).catch((error: unknown) => ctx.notify("error", errorText(error)));
              },
            },
          });
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("pos:hold.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="hold" variant="primary" disabled={hold.isPending}>
            {t("pos:hold.submit")}
          </Button>
        </>
      }
    >
      <form id="hold" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <Field id="hold-label" label={t("pos:hold.label")}>
          <Input
            id="hold-label"
            autoFocus
            value={label}
            maxLength={60}
            onChange={(e) => setLabel(e.target.value)}
          />
        </Field>
        {hold.error ? (
          <p role="alert" className="text-danger">
            {errorText(hold.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
