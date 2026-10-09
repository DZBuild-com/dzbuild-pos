import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Sheet } from "@/components/ui/sheet";
import { formatDate } from "@/lib/format";
import { useRevokeTill, useTillClosures, useTills } from "@/lib/ipc/lan";
import type { Session, TillInfo } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** `sync::SKEW_WARN_MS`: beyond 5 minutes the clock column warns. */
const SKEW_WARN_MS = 5 * 60_000;

/** LAN-004: the main PC's tills — what each still has to send, its clock and its journal; their
 * Z, and the owner's "Retirer" (revocation). */
export function TillsTable({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const tills = useTills();
  const [zOf, setZOf] = useState<TillInfo | null>(null);
  const [revoking, setRevoking] = useState<TillInfo | null>(null);
  const canSeeZ = session.permissions.includes("cash.view_history");
  const rows = tills.data ?? [];
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("sync:lan.tills.title")}</h2>
      {tills.isSuccess && rows.length === 0 ? (
        <p className="text-ink-2">{t("sync:lan.tills.empty")}</p>
      ) : null}
      {rows.length > 0 ? (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-start">
            <thead className="text-sm text-ink-2">
              <tr className="h-10">
                <th className="text-start font-semibold">{t("sync:lan.tills.code")}</th>
                <th className="text-start font-semibold">{t("sync:lan.tills.name")}</th>
                <th className="text-start font-semibold">{t("sync:lan.tills.state")}</th>
                <th className="text-start font-semibold">{t("sync:lan.tills.lastSeen")}</th>
                <th className="text-start font-semibold">{t("sync:lan.tills.pending")}</th>
                <th className="text-start font-semibold">{t("sync:lan.tills.version")}</th>
                <th className="text-start font-semibold">{t("sync:lan.tills.clock")}</th>
                <th className="text-start font-semibold">{t("sync:lan.tills.journal")}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((till) => {
                const skewOff = till.skewMs !== null && Math.abs(till.skewMs) > SKEW_WARN_MS;
                return (
                  <tr key={till.terminalId} className="h-12 border-t border-line">
                    <td className="pe-3 font-mono font-semibold">
                      <bdi dir="ltr">{till.code}</bdi>
                    </td>
                    <td className="pe-3">
                      <bdi>{till.name}</bdi>
                    </td>
                    <td className="pe-3">
                      <div className="flex flex-col items-start gap-1">
                        {till.revoked ? (
                          <Badge tone="neutral">{t("sync:lan.tills.revoked")}</Badge>
                        ) : till.online ? (
                          <Badge tone="ok">{t("sync:lan.tills.online")}</Badge>
                        ) : (
                          <Badge tone="warn">{t("sync:lan.tills.offline")}</Badge>
                        )}
                        {till.pinsFeed ? (
                          <span className="text-sm text-warn" title={t("sync:lan.tills.pinsFeedHelp")}>
                            {t("sync:lan.tills.pinsFeed")}
                          </span>
                        ) : null}
                      </div>
                    </td>
                    <td className="pe-3">
                      {till.lastSeenAt === null
                        ? t("sync:lan.tills.never")
                        : formatDate(till.lastSeenAt, lang, { withTime: true })}
                    </td>
                    <td className="pe-3">{t("sync:lan.toSend", { count: till.outboxPending })}</td>
                    <td className="pe-3">
                      <bdi dir="ltr">{till.appVersion ?? "—"}</bdi>
                    </td>
                    <td className="pe-3">
                      {skewOff ? (
                        <Badge tone="warn">
                          {t("sync:lan.tills.clockOff", {
                            minutes: Math.round(Math.abs(till.skewMs ?? 0) / 60_000),
                          })}
                        </Badge>
                      ) : (
                        t("sync:lan.tills.clockOk")
                      )}
                    </td>
                    <td className="pe-3">
                      {till.journalIntact ? (
                        <Badge tone="ok">{t("sync:lan.tills.journalOk")}</Badge>
                      ) : (
                        <Badge tone="danger">{t("sync:lan.tills.journalBroken")}</Badge>
                      )}
                    </td>
                    <td className="py-1">
                      <div className="flex items-center justify-end gap-2">
                        {canSeeZ ? (
                          <Button variant="outline" onClick={() => setZOf(till)}>
                            {t("sync:lan.tills.zList")}
                          </Button>
                        ) : null}
                        {session.isOwner && !till.revoked ? (
                          <Button variant="danger" onClick={() => setRevoking(till)}>
                            {t("sync:lan.tills.revoke")}
                          </Button>
                        ) : null}
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      ) : null}
      {zOf ? <TillZSheet till={zOf} onClose={() => setZOf(null)} /> : null}
      {revoking ? <RevokeDialog till={revoking} onClose={() => setRevoking(null)} /> : null}
    </Card>
  );
}

/** The Z a till has sent, newest first; a Z closed with operations still to send says so. */
function TillZSheet({ till, onClose }: { till: TillInfo; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const closures = useTillClosures(till.terminalId);
  const list = closures.data ?? [];
  return (
    <Sheet
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("sync:lan.zList.title", { code: till.code })}
    >
      {closures.isSuccess && list.length === 0 ? (
        <p className="text-ink-2">{t("sync:lan.zList.empty")}</p>
      ) : null}
      <ul className="flex flex-col gap-2">
        {list.map((z) => (
          <li key={z.sessionId} className="flex flex-col gap-1 rounded-card border border-line p-3">
            <div className="flex items-center justify-between gap-2">
              <bdi dir="ltr" className="font-mono font-bold">
                {z.zNumber}
              </bdi>
              {z.offlinePending !== null ? <Badge tone="warn">{t("sync:lan.zList.offline")}</Badge> : null}
            </div>
            <span className="text-sm text-ink-2">
              {t("sync:lan.zList.closedAt", {
                date: formatDate(z.closedAt, lang, { withTime: true }),
                name: z.closedByName,
              })}
            </span>
            <div className="flex justify-between gap-2">
              <span>{t("sync:lan.zList.net")}</span>
              <Amount value={z.netTtc} className="font-semibold" />
            </div>
            <div className="flex justify-between gap-2">
              <span>{t("sync:lan.zList.variance")}</span>
              <Amount value={z.variance} className={z.variance === 0 ? "" : "text-danger"} />
            </div>
          </li>
        ))}
      </ul>
    </Sheet>
  );
}

/** Revocation (LAN-004): the till's token stops working; what it already sent stays. */
function RevokeDialog({ till, onClose }: { till: TillInfo; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const revoke = useRevokeTill();
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("sync:lan.tills.revokeTitle", { name: till.name })}
      description={t("sync:lan.tills.revokeBody")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="danger"
            disabled={revoke.isPending}
            onClick={() => revoke.mutate(till.terminalId, { onSuccess: onClose })}
          >
            {t("sync:lan.tills.revokeConfirm")}
          </Button>
        </>
      }
    >
      {till.outboxPending > 0 ? (
        <p role="alert" className="rounded-control bg-warn-soft p-3">
          {t("sync:lan.tills.revokePending", { count: till.outboxPending })}
        </p>
      ) : null}
      {till.pinsFeed ? <p>{t("sync:lan.tills.pinsFeedHelp")}</p> : null}
      {revoke.error ? (
        <p role="alert" className="text-danger">
          {errorText(revoke.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
