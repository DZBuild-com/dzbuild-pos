import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ManagerApprovalDialog, useApproval } from "@/components/manager-approval";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { ApprovalCancelled, isApprovalCancelled } from "@/lib/approval";
import { useCompLine, useVoidLine } from "@/lib/ipc/restaurant";
import type { CompReason, Permission, TicketLine, VoidReason } from "@/lib/ipc/types";
import { formatQty, parseQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const VOID_REASONS: VoidReason[] = ["mistake", "customer_changed", "quality", "other"];
const COMP_REASONS: CompReason[] = ["offered", "staff", "quality", "other"];

interface Asking {
  permission: Permission;
  approved: () => void;
  cancelled: () => void;
}

/** M1b's manager approval for an action the user's role lacks: asked first (a waiter voiding a
 * sent dish), the grant then waits in Rust for the action. Any other refusal a manager can lift
 * is asked after, as at the till (`useApproval`). */
function useApprovedRun(holds: boolean, permission: Permission) {
  const approval = useApproval();
  const [asking, setAsking] = useState<Asking | null>(null);
  /** `false` when the manager's approval was cancelled. */
  const run = async (action: () => Promise<unknown>): Promise<boolean> => {
    try {
      if (!holds) {
        await new Promise<void>((resolve, reject) =>
          setAsking({ permission, approved: resolve, cancelled: () => reject(new ApprovalCancelled()) }),
        );
        setAsking(null);
      }
      await approval.run(action);
      return true;
    } catch (error) {
      setAsking(null);
      if (isApprovalCancelled(error)) return false;
      throw error;
    }
  };
  const dialog = (
    <>
      {asking ? (
        <ManagerApprovalDialog
          permission={asking.permission}
          onApproved={asking.approved}
          onCancel={asking.cancelled}
        />
      ) : null}
      {approval.dialog}
    </>
  );
  return { run, dialog, pending: asking !== null || approval.pending };
}

/** RST-008: voids all or part of a line the kitchen received, with a reason (the kitchen gets a
 * cancellation ticket); a waiter without `sales.void_line` needs a manager. */
export function VoidDialog({
  line,
  may,
  onClose,
}: {
  line: TicketLine;
  may: (permission: Permission) => boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const approval = useApprovedRun(may("sales.void_line"), "sales.void_line");
  const voidLine = useVoidLine();
  const [reason, setReason] = useState<VoidReason | "">("");
  const [qty, setQty] = useState(formatQty(line.qty, lang));
  const [error, setError] = useState<unknown>(null);
  const parsed = parseQty(qty, 3);
  const valid = reason !== "" && parsed !== null && parsed > 0 && parsed <= line.qty;
  const submit = async () => {
    if (!valid) return;
    setError(null);
    try {
      const done = await approval.run(() =>
        voidLine.mutateAsync({ lineId: line.id, qty: parsed === line.qty ? null : parsed, reason }),
      );
      if (done) onClose();
    } catch (e) {
      setError(e);
    }
  };
  return (
    <>
      <Dialog
        open={!approval.pending}
        onOpenChange={(open) => (open ? undefined : onClose())}
        title={t("restaurant:void.title")}
        description={line.name}
        footer={
          <>
            <Button variant="outline" size="lg" onClick={onClose}>
              {t("common:actions.cancel")}
            </Button>
            <Button
              variant="danger"
              size="lg"
              disabled={!valid || voidLine.isPending}
              onClick={() => void submit()}
            >
              {t("restaurant:void.submit")}
            </Button>
          </>
        }
      >
        <Field id="void-qty" label={t("restaurant:void.qty")}>
          <NumericInput id="void-qty" kind="qty" value={qty} onValueChange={setQty} />
        </Field>
        <RadioCards<VoidReason | "">
          legend={t("restaurant:void.reason")}
          name="void-reason"
          value={reason}
          onChange={setReason}
          options={VOID_REASONS.map((r) => ({ value: r, label: t(`restaurant:void.reasons.${r}`) }))}
        />
        {error ? (
          <p role="alert" className="text-danger">
            {errorText(error)}
          </p>
        ) : null}
      </Dialog>
      {approval.dialog}
    </>
  );
}

/** RST-008: offers a line (100 % off) with its reason; the discount right or a manager's approval. */
export function CompDialog({
  line,
  may,
  onClose,
}: {
  line: TicketLine;
  may: (permission: Permission) => boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const approval = useApprovedRun(may("sales.discount"), "sales.discount");
  const comp = useCompLine();
  const [reason, setReason] = useState<CompReason | "">("");
  const [error, setError] = useState<unknown>(null);
  const submit = async () => {
    if (reason === "") return;
    setError(null);
    try {
      const done = await approval.run(() => comp.mutateAsync({ lineId: line.id, reason }));
      if (done) onClose();
    } catch (e) {
      setError(e);
    }
  };
  return (
    <>
      <Dialog
        open={!approval.pending}
        onOpenChange={(open) => (open ? undefined : onClose())}
        title={t("restaurant:comp.title")}
        description={line.name}
        footer={
          <>
            <Button variant="outline" size="lg" onClick={onClose}>
              {t("common:actions.cancel")}
            </Button>
            <Button
              variant="primary"
              size="lg"
              disabled={reason === "" || comp.isPending}
              onClick={() => void submit()}
            >
              {t("restaurant:ticket.comp")}
            </Button>
          </>
        }
      >
        <RadioCards<CompReason | "">
          legend={t("restaurant:comp.reason")}
          name="comp-reason"
          value={reason}
          onChange={setReason}
          options={COMP_REASONS.map((r) => ({ value: r, label: t(`restaurant:comp.reasons.${r}`) }))}
        />
        {error ? (
          <p role="alert" className="text-danger">
            {errorText(error)}
          </p>
        ) : null}
      </Dialog>
      {approval.dialog}
    </>
  );
}
