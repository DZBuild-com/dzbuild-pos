import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { isApprovalCancelled } from "@/lib/approval";
import { useOpenDrawer } from "@/lib/ipc/hardware";
import type { NoSaleReason } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import type { PosContext } from "./PosDialogs";

/** F9 (AUD-009, HW-040): the drawer opens without a sale only with a reason, journaled. */
export function NoSaleDialog({ ctx, onClose }: { ctx: PosContext; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const open = useOpenDrawer();
  const [reason, setReason] = useState<NoSaleReason>("change_request");
  const [note, setNote] = useState("");
  const [error, setError] = useState<unknown>(null);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await ctx.run(() => open.mutateAsync({ reason, note: note.trim() === "" ? null : note.trim() }));
      toast.success(t("pos:drawer.done"));
      onClose();
    } catch (e) {
      if (!isApprovalCancelled(e)) setError(e);
    }
  };
  return (
    <Dialog
      open
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("pos:drawer.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="no-sale" variant="primary" disabled={open.isPending}>
            {t("pos:drawer.submit")}
          </Button>
        </>
      }
    >
      <form id="no-sale" noValidate onSubmit={(e) => void submit(e)} className="flex flex-col gap-4">
        <RadioCards
          legend={t("pos:drawer.reason")}
          name="no-sale-reason"
          value={reason}
          onChange={setReason}
          options={(["change_request", "check", "other"] as const).map((r) => ({
            value: r,
            label: t(`pos:drawer.reasons.${r}`),
          }))}
        />
        <Field
          id="no-sale-note"
          label={t("pos:drawer.note")}
          optional={reason === "other" ? undefined : t("setup:shop.optional")}
        >
          <Input id="no-sale-note" value={note} maxLength={200} onChange={(e) => setNote(e.target.value)} />
        </Field>
        {error ? (
          <p role="alert" className="text-danger">
            {errorText(error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
