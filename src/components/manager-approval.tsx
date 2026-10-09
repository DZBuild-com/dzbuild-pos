import { ShieldCheck } from "lucide-react";
import { type ReactNode, useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { Avatar } from "@/components/avatar";
import { PinPad } from "@/components/pin-pad";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { ApprovalCancelled, approvalPermission } from "@/lib/approval";
import { useManagerOverride, useSession, useUserTiles } from "@/lib/ipc/auth";
import { toAppError } from "@/lib/ipc/client";
import type { OverrideGrant, Permission, UserTile } from "@/lib/ipc/types";
import { beep } from "@/lib/sounds";
import { useErrorText } from "@/lib/use-error-text";

export { isApprovalCancelled } from "@/lib/approval";

/** What the manager is about to lift, in the backend's figures: a credit sale above the
 * customer's limit shows his balance, his limit and the balance the sale would leave (CRD-002). */
function Refusal({ error }: { error: unknown }) {
  const { t } = useTranslation();
  const e = toAppError(error);
  if (e.code !== "credit_limit_exceeded") return null;
  const details = (e.details ?? {}) as Record<string, unknown>;
  const figures = (
    [
      ["customers:detail.balance", details.balance],
      ["customers:detail.limit", details.limit],
      ["auth:approval.balanceAfter", details.after],
    ] as [string, unknown][]
  ).flatMap(([label, value]) => (typeof value === "number" ? [{ label, value }] : []));
  return (
    <div className="mb-3 flex flex-col gap-2 rounded-control bg-warn-soft p-3 text-warn">
      <p className="font-semibold">{t("auth:approval.limitExceeded")}</p>
      {figures.length > 0 ? (
        <dl className="grid gap-1">
          {figures.map(({ label, value }) => (
            <div key={label} className="flex justify-between gap-3">
              <dt>{t(label)}</dt>
              <dd>
                <Amount value={value} className="font-bold" />
              </dd>
            </div>
          ))}
        </dl>
      ) : null}
    </div>
  );
}

/** A manager chooses their tile and types their PIN (spec §4.5); the grant is kept in Rust.
 * `refusal` is the backend's answer that asked for it, shown when it carries figures. */
export function ManagerApprovalDialog({
  permission,
  refusal,
  onApproved,
  onCancel,
}: {
  permission: Permission;
  refusal?: unknown;
  onApproved: (grant: OverrideGrant) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const session = useSession().data;
  const tiles = useUserTiles();
  const override = useManagerOverride();
  const [approver, setApprover] = useState<UserTile | null>(null);
  const [pin, setPin] = useState("");
  const others = (tiles.data ?? []).filter((tile) => tile.id !== session?.userId);
  const submit = () => {
    if (!approver) return;
    override.mutate(
      { approverId: approver.id, pin, permission },
      {
        onSuccess: (grant) => {
          toast.success(t("auth:approval.granted", { name: grant.approverName }));
          onApproved(grant);
        },
        onError: () => setPin(""),
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
      title={t("auth:approval.title")}
      description={t("auth:approval.why", { permission: t(`auth:permissions.${permission}`) })}
      footer={
        <Button variant="outline" onClick={onCancel}>
          {t("common:actions.cancel")}
        </Button>
      }
    >
      <Refusal error={refusal} />
      {approver === null ? (
        <div className="flex flex-col gap-3">
          <p className="font-semibold">{t("auth:approval.chooseApprover")}</p>
          {others.length === 0 && !tiles.isPending ? (
            <p className="text-ink-2">{t("auth:approval.noApprover")}</p>
          ) : null}
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
            {others.map((tile) => (
              <button
                key={tile.id}
                type="button"
                onClick={() => setApprover(tile)}
                className="flex min-h-16 items-center gap-3 rounded-card border border-line bg-surface p-3 text-start font-semibold hover:bg-surface-2"
              >
                <Avatar name={tile.displayName} />
                {tile.displayName}
              </button>
            ))}
          </div>
        </div>
      ) : (
        <div className="flex flex-col items-center gap-3">
          <p className="flex items-center gap-2 font-semibold">
            <ShieldCheck aria-hidden className="size-5" />
            {t("auth:approval.pin", { name: approver.displayName })}
          </p>
          <PinPad
            value={pin}
            onChange={setPin}
            onSubmit={submit}
            onCancel={() => setApprover(null)}
            submitLabel={t("auth:approval.submit")}
            disabled={override.isPending}
          />
          {override.error ? (
            <p role="alert" className="text-danger">
              {errorText(override.error)}
            </p>
          ) : null}
        </div>
      )}
    </Dialog>
  );
}

interface Pending {
  permission: Permission;
  refusal: unknown;
  resolve: () => void;
  reject: () => void;
}

/** Runs a till action; when the backend refuses something a manager can lift, beeps, asks for the
 * manager's PIN and retries (at most twice: a discount then a price override). */
export function useApproval(): {
  run: <T>(action: () => Promise<T>) => Promise<T>;
  dialog: ReactNode;
  pending: boolean;
} {
  const [pending, setPending] = useState<Pending | null>(null);
  const run = useCallback(async <T,>(action: () => Promise<T>): Promise<T> => {
    for (let attempt = 0; ; attempt += 1) {
      try {
        return await action();
      } catch (error) {
        const permission = approvalPermission(error);
        if (permission === null || attempt >= 2) throw error;
        beep("error");
        await new Promise<void>((resolve, reject) => {
          setPending({
            permission,
            refusal: error,
            resolve,
            reject: () => reject(new ApprovalCancelled()),
          });
        });
        setPending(null);
      }
    }
  }, []);
  const dialog = pending ? (
    <ManagerApprovalDialog
      permission={pending.permission}
      refusal={pending.refusal}
      onApproved={() => pending.resolve()}
      onCancel={() => {
        pending.reject();
        setPending(null);
      }}
    />
  ) : null;
  return { run, dialog, pending: pending !== null };
}
