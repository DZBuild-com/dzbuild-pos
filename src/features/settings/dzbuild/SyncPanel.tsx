import { RefreshCw, Unlink } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { PinInput } from "@/components/pin-pad";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { formatDate } from "@/lib/format";
import {
  useCloudNotices,
  useDismissNotice,
  useFailedItems,
  useOpenCloudPage,
  useRetryItem,
  useSyncNow,
  useUnlink,
} from "@/lib/ipc/cloud";
import type { CloudNotice, Session, SyncStatus } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** A notice as one sentence: `sync:notices.<code>` filled with its subject (an endpoint or an
 * outbox kind by name, else as DZBuild or the till wrote it: an order or document number) and
 * its details; a DZBuild event type is said by name (`sync:events.<type>`, else "a change"). */
function useNoticeText(): (notice: CloudNotice) => string {
  const { t, i18n } = useTranslation();
  const errorText = useErrorText();
  const eventName = (type: unknown) =>
    typeof type === "string" && i18n.exists(`sync:events.${type}`)
      ? t(`sync:events.${type}`)
      : t("sync:events.other");
  return (n) => {
    const subjectKey = n.subject
      ? [`sync:endpoints.${n.subject}`, `sync:entities.kinds.${n.subject}`].find((k) => i18n.exists(k))
      : undefined;
    const values: Record<string, unknown> = {
      subject: subjectKey ? t(subjectKey) : n.subject,
      count: n.count,
    };
    if (n.details && typeof n.details === "object" && !Array.isArray(n.details)) {
      for (const [key, value] of Object.entries(n.details)) {
        values[key] = Array.isArray(value) ? value.join(", ") : value;
      }
      // a refused item's `reason` is the refusal's error code: said as its sentence
      if (typeof n.details.reason === "string") values.reason = errorText({ code: n.details.reason });
    }
    const details = n.details && typeof n.details === "object" && !Array.isArray(n.details) ? n.details : {};
    // the event a refusal is about: its type in the details, or the subject when it is the type
    values.event = eventName(details.type ?? n.subject);
    // a revoked device and a refused refresh share their code; the kind tells them apart
    if (n.kind === "revoked") return t("sync:notices.cloud_revoked", values);
    // the plan refused a whole kind (subject: its endpoint) or one event's detail, nothing drafted
    if (n.code === "cloud_plan_cap" && details.drafted === undefined) {
      return t(
        details.type === undefined ? "sync:notices.cloud_plan_cap_kind" : "sync:notices.cloud_plan_cap_event",
        values,
      );
    }
    const key = `sync:notices.${n.code}`;
    return i18n.exists(key) ? t(key, values) : errorText({ code: n.code });
  };
}

export function SyncPanel({ status, session }: { status: SyncStatus; session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const noticeText = useNoticeText();
  const now = useSyncNow();
  const open = useOpenCloudPage();
  const notices = useCloudNotices();
  const dismiss = useDismissNotice();
  const failed = useFailedItems();
  const retry = useRetryItem();
  const [unlinking, setUnlinking] = useState(false);
  const state = status.linkStatus === "linked" ? (status.online ? "online" : "offline") : status.linkStatus;
  const onError = (error: unknown) => toast.error(errorText(error));
  // the card appears whole, with what the owner must know, rather than growing under the eyes
  if (!notices.isFetched || !failed.isFetched) return null;
  return (
    <Card>
      <div className="flex flex-wrap items-center gap-3">
        <div className="grow">
          <p className="text-xl font-bold">{status.storeName}</p>
          <p className="text-sm text-ink-2">
            {t(`sync:status.${state ?? "offline"}`)}
            {status.lastSuccessAt ? (
              <>
                {" · "}
                {t("sync:status.lastSync", {
                  at: formatDate(status.lastSuccessAt, lang, { withTime: true }),
                })}
              </>
            ) : null}
          </p>
        </div>
        {status.publicUrl ? (
          <Button variant="ghost" onClick={() => open.mutate("store", { onError })}>
            {t("sync:status.openStore")}
          </Button>
        ) : null}
        <Button
          variant="outline"
          disabled={now.isPending}
          onClick={() =>
            now.mutate(undefined, { onSuccess: () => toast.success(t("sync:status.syncing")), onError })
          }
        >
          <RefreshCw aria-hidden />
          {t("sync:status.syncNow")}
        </Button>
      </div>
      {status.linkStatus === "relink_required" || status.linkStatus === "revoked" ? (
        <p role="alert" className="rounded-control bg-warn-soft p-3">
          {t(`sync:status.${status.linkStatus}Help`)}
        </p>
      ) : null}
      {status.entities.length > 0 ? (
        <table className="w-full border-collapse">
          <thead className="text-sm text-ink-2">
            <tr className="h-10">
              <th className="text-start font-semibold">{t("sync:entities.what")}</th>
              <th className="text-end font-semibold">{t("sync:entities.pending")}</th>
              <th className="text-end font-semibold">{t("sync:entities.failed")}</th>
              <th className="text-end font-semibold">{t("sync:entities.lastSent")}</th>
            </tr>
          </thead>
          <tbody className="tabular-nums">
            {status.entities.map((e) => (
              <tr key={e.kind} className="h-11 border-t border-line">
                <td>{t(`sync:entities.kinds.${e.kind}`)}</td>
                <td className="text-end">{e.pending}</td>
                <td className={e.failed > 0 ? "text-end text-danger" : "text-end"}>{e.failed}</td>
                <td className="text-end">
                  {e.lastSentAt ? (
                    <bdi dir="ltr">{formatDate(e.lastSentAt, lang, { withTime: true })}</bdi>
                  ) : (
                    "—"
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
      {(notices.data ?? []).length > 0 ? (
        <section aria-labelledby="cloud-notices" className="flex flex-col gap-2">
          <h4 id="cloud-notices" className="font-semibold">
            {t("sync:notices.title")}
          </h4>
          {notices.data?.map((n) => (
            <div key={n.id} className="flex items-start gap-3 rounded-control bg-surface-2 p-3">
              <p className="grow">{noticeText(n)}</p>
              <Button
                variant="ghost"
                disabled={dismiss.isPending}
                onClick={() => dismiss.mutate(n.id, { onError })}
              >
                {t("sync:notices.dismiss")}
              </Button>
            </div>
          ))}
        </section>
      ) : null}
      {(failed.data ?? []).length > 0 ? (
        <section aria-labelledby="cloud-failed" className="flex flex-col gap-2">
          <h4 id="cloud-failed" className="font-semibold">
            {t("sync:failed.title")}
          </h4>
          <p className="text-sm text-ink-2">{t("sync:failed.help")}</p>
          {failed.data?.map((f) => (
            <div key={f.id} className="flex items-center gap-3 border-t border-line py-2">
              <div className="grow">
                <p>{f.label ?? t(`sync:entities.kinds.${f.kind}`)}</p>
                <p className="text-sm text-ink-2">{errorText({ code: f.code })}</p>
              </div>
              <Button
                variant="outline"
                disabled={retry.isPending}
                onClick={() => retry.mutate(f.id, { onError })}
              >
                {t("sync:failed.retry")}
              </Button>
            </div>
          ))}
        </section>
      ) : null}
      <div className="flex justify-end">
        <Button
          variant="ghost"
          disabled={!session.isOwner}
          title={session.isOwner ? undefined : t("sync:link.ownerOnly")}
          onClick={() => setUnlinking(true)}
        >
          <Unlink aria-hidden />
          {t("sync:unlink.button")}
        </Button>
      </div>
      {unlinking ? (
        <UnlinkDialog pending={status.pending + status.failed} onClose={() => setUnlinking(false)} />
      ) : null}
    </Card>
  );
}

/** Owner + PIN: DZBuild revokes the access and forgets this till (best effort offline); the data
 * stays on this PC, the `pending` uploads are dropped (a relink of the same store keeps them). */
function UnlinkDialog({ pending, onClose }: { pending: number; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const unlink = useUnlink();
  const [pin, setPin] = useState("");
  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      title={t("sync:unlink.title")}
      description={t("sync:unlink.body", { pending })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="danger"
            disabled={pin.length < 4 || unlink.isPending}
            onClick={() =>
              unlink.mutate(pin, {
                onSuccess: (outcome) => {
                  toast.success(t(outcome.revoked ? "sync:unlink.done" : "sync:unlink.doneOffline"));
                  onClose();
                },
              })
            }
          >
            {t("sync:unlink.confirm")}
          </Button>
        </>
      }
    >
      <Field id="unlink-pin" label={t("sync:unlink.pin")}>
        <PinInput id="unlink-pin" value={pin} onChange={setPin} />
      </Field>
      {unlink.error ? (
        <p role="alert" className="text-danger">
          {errorText(unlink.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
