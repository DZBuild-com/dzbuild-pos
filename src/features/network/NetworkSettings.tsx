import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { useDemoStatus } from "@/lib/ipc/demo";
import { useBecomeHost, useBecomeStandalone, useLanStatus } from "@/lib/ipc/lan";
import type { LanStatus, Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { HostPanel } from "./HostPanel";
import { JoinWizard } from "./JoinWizard";
import { PendingEntriesCard } from "./PendingEntriesCard";
import { TillPanel } from "./TillPanel";

/** Settings › Réseau (LAN-001..012): what this PC is on the shop's network, and what it serves. */
export function NetworkSettings({ session }: { session: Session }) {
  const errorText = useErrorText();
  const status = useLanStatus();
  if (status.isPending) return null;
  if (status.isError) {
    return (
      <p role="alert" className="text-danger">
        {errorText(status.error)}
      </p>
    );
  }
  const lan = status.data;
  return (
    <div className="flex flex-col gap-6">
      <RoleCard session={session} status={lan} />
      {lan.role === "host" && lan.host ? <HostPanel session={session} host={lan.host} /> : null}
      {lan.role === "client" && lan.client ? <TillPanel session={session} status={lan.client} /> : null}
      {lan.role === "client" && session.permissions.includes("settings.data") ? <PendingEntriesCard /> : null}
    </div>
  );
}

/** The role and, for the owner, the way to change it: a single PC becomes the main PC or joins
 * one; the main PC goes back to a single PC once no till is paired. */
function RoleCard({ session, status }: { session: Session; status: LanStatus }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const becomeHost = useBecomeHost();
  const becomeStandalone = useBecomeStandalone();
  const [confirming, setConfirming] = useState(false);
  const [joining, setJoining] = useState(false);
  // The demo shop joins no network and becomes no main PC (the real shop's files).
  const demo = useDemoStatus().data?.active === true;
  const tillsPaired = (status.host?.tillsTotal ?? 0) > 0;
  const error = becomeHost.error ?? becomeStandalone.error;
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("sync:lan.role.title")}</h2>
      <p className="text-lg font-semibold">{t(`sync:lan.role.${status.role}`)}</p>
      <p className="text-ink-2">{t(`sync:lan.role.${status.role}Help`)}</p>
      {demo ? (
        <p role="note" className="rounded-control bg-info-soft p-3">
          {t("sync:lan.role.demo")}
        </p>
      ) : !session.isOwner ? (
        <p className="text-ink-2">{t("sync:lan.role.ownerOnly")}</p>
      ) : status.role === "standalone" ? (
        <div className="flex flex-wrap gap-2">
          <Button variant="primary" onClick={() => setConfirming(true)}>
            {t("sync:lan.role.makeHost")}
          </Button>
          <Button variant="outline" onClick={() => setJoining(true)}>
            {t("sync:lan.role.joinHost")}
          </Button>
        </div>
      ) : status.role === "host" ? (
        <div className="flex flex-col items-start gap-1">
          <Button
            variant="outline"
            disabled={tillsPaired || becomeStandalone.isPending}
            onClick={() => becomeStandalone.mutate()}
          >
            {t("sync:lan.role.backToSingle")}
          </Button>
          {tillsPaired ? (
            <p className="text-sm text-ink-2">{t("sync:lan.role.backToSingleBlocked")}</p>
          ) : null}
        </div>
      ) : null}
      {error ? (
        <p role="alert" className="text-danger">
          {errorText(error)}
        </p>
      ) : null}
      <Dialog
        open={confirming}
        onOpenChange={setConfirming}
        title={t("sync:lan.role.makeHostTitle")}
        description={t("sync:lan.role.makeHostBody")}
        footer={
          <>
            <Button variant="outline" onClick={() => setConfirming(false)}>
              {t("common:actions.cancel")}
            </Button>
            <Button
              variant="primary"
              disabled={becomeHost.isPending}
              onClick={() => becomeHost.mutate(undefined, { onSettled: () => setConfirming(false) })}
            >
              {t("sync:lan.role.makeHostConfirm")}
            </Button>
          </>
        }
      >
        {null}
      </Dialog>
      <Dialog open={joining} onOpenChange={setJoining} title={t("sync:lan.join.title")} size="lg">
        <JoinWizard titled={false} onBack={() => setJoining(false)} />
      </Dialog>
    </Card>
  );
}
