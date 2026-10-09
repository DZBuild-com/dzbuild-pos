import { RefreshCw } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { PIN_MIN, PinInput } from "@/components/pin-pad";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { formatDate } from "@/lib/format";
import { useLeave, usePromote, useRetryRejected, useSyncNow } from "@/lib/ipc/lan";
import type { ClientStatus, Session } from "@/lib/ipc/types";
import { LINK_TONE } from "./link-tone";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { JoinWizard } from "./JoinWizard";

/** Why a till cannot sync at all until it is paired again: its token is refused, missing or
 * unreadable. Its unsent operations go with the new pairing (the join replays them first). */
const NEEDS_PAIRING = new Set(["lan_token_invalid", "lan_not_paired", "lan_vault_unavailable"]);

/** Settings › Réseau on a till (LAN-005/010): the link, what is left to send, and the owner's
 * two exits (make this till the main PC, leave the network), each confirmed with the PIN. */
export function TillPanel({ session, status }: { session: Session; status: ClientStatus }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const sync = useSyncNow();
  const retry = useRetryRejected();
  const [exit, setExit] = useState<"promote" | "leave" | null>(null);
  const [rejoining, setRejoining] = useState(false);
  const actionError = retry.error ?? sync.error;
  const cause = status.rejected === 0 && status.link === "attention" ? status.lastError : null;
  const needsPairing = cause !== null && NEEDS_PAIRING.has(cause);
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("sync:lan.till.title")}</h2>
      <p>{t("sync:lan.till.host", { name: status.hostName, address: status.hostAddress })}</p>
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone={LINK_TONE[status.link]}>{t(`sync:lan.link.${status.link}`)}</Badge>
        <span>{t("sync:lan.toSend", { count: status.pending })}</span>
      </div>
      <p className="text-ink-2">
        {t("sync:lan.till.lastContact", {
          time:
            status.lastContactAt === null
              ? t("sync:lan.tills.never")
              : formatDate(status.lastContactAt, lang, { withTime: true }),
        })}
      </p>
      {status.skewMs !== null ? (
        <p className="text-ink-2">
          {t("sync:lan.till.clock", { seconds: Math.round(status.skewMs / 1_000) })}
        </p>
      ) : null}
      {cause !== null ? (
        <div role="alert" className="flex flex-col gap-2 rounded-control bg-danger-soft p-3 text-danger">
          <p className="font-semibold">{errorText({ code: cause })}</p>
          {needsPairing ? <p>{t("sync:lan.till.rejoinHelp")}</p> : null}
          {needsPairing && session.isOwner ? (
            <div>
              <Button variant="danger" onClick={() => setRejoining(true)}>
                {t("sync:lan.till.rejoin")}
              </Button>
            </div>
          ) : null}
        </div>
      ) : null}
      {status.rejected > 0 ? (
        <div role="alert" className="flex flex-col gap-2 rounded-control bg-danger-soft p-3 text-danger">
          <p className="font-semibold">{t("sync:lan.till.rejected", { count: status.rejected })}</p>
          {status.lastError ? <p>{errorText({ code: status.lastError })}</p> : null}
          <div>
            <Button variant="danger" disabled={retry.isPending} onClick={() => retry.mutate()}>
              {t("sync:lan.till.retry")}
            </Button>
          </div>
        </div>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <Button variant="outline" disabled={sync.isPending} onClick={() => sync.mutate()}>
          <RefreshCw aria-hidden />
          {t("sync:lan.till.syncNow")}
        </Button>
        {session.isOwner ? (
          <>
            {needsPairing ? null : (
              <Button variant="outline" onClick={() => setRejoining(true)}>
                {t("sync:lan.till.rejoin")}
              </Button>
            )}
            <Button variant="outline" onClick={() => setExit("promote")}>
              {t("sync:lan.till.promote")}
            </Button>
            <Button variant="danger" onClick={() => setExit("leave")}>
              {t("sync:lan.till.leave")}
            </Button>
          </>
        ) : null}
      </div>
      {actionError ? (
        <p role="alert" className="text-danger">
          {errorText(actionError)}
        </p>
      ) : null}
      {exit ? <ExitDialog kind={exit} onClose={() => setExit(null)} /> : null}
      <Dialog open={rejoining} onOpenChange={setRejoining} title={t("sync:lan.till.rejoin")} size="lg">
        <JoinWizard titled={false} onBack={() => setRejoining(false)} />
      </Dialog>
    </Card>
  );
}

function ExitDialog({ kind, onClose }: { kind: "promote" | "leave"; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const promote = usePromote();
  const leave = useLeave();
  const action = kind === "promote" ? promote : leave;
  const [pin, setPin] = useState("");
  const restarting = action.isSuccess;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !restarting) onClose();
      }}
      dismissible={!restarting}
      title={t(kind === "promote" ? "sync:lan.till.promoteTitle" : "sync:lan.till.leaveTitle")}
      description={t(kind === "promote" ? "sync:lan.till.promoteBody" : "sync:lan.till.leaveBody")}
      footer={
        restarting ? null : (
          <>
            <Button variant="outline" onClick={onClose}>
              {t("common:actions.cancel")}
            </Button>
            <Button
              variant={kind === "promote" ? "primary" : "danger"}
              disabled={pin.length < PIN_MIN || action.isPending}
              onClick={() => action.mutate(pin)}
            >
              {t(kind === "promote" ? "sync:lan.till.promoteConfirm" : "sync:lan.till.leaveConfirm")}
            </Button>
          </>
        )
      }
    >
      {kind === "promote" && !restarting ? <p>{t("sync:lan.till.promoteDzbuild")}</p> : null}
      {restarting ? (
        <p role="status">{t("sync:lan.till.restarting")}</p>
      ) : (
        <Field id="lan-exit-pin" label={t("sync:lan.till.pin")}>
          <PinInput id="lan-exit-pin" value={pin} onChange={setPin} />
        </Field>
      )}
      {action.error ? (
        <p role="alert" className="text-danger">
          {errorText(action.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
