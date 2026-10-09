import { DatabaseBackup, OctagonAlert, TriangleAlert } from "lucide-react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { formatDate } from "@/lib/format";
import { useBackupStatus } from "@/lib/ipc/backup";
import type { Session } from "@/lib/ipc/types";
import { cn } from "@/lib/utils";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

function Banner({
  tone,
  title,
  children,
  action,
}: {
  tone: "warn" | "danger";
  title: string;
  children: ReactNode;
  action: ReactNode;
}) {
  const Icon = tone === "danger" ? OctagonAlert : TriangleAlert;
  return (
    <div
      role="alert"
      className={cn(
        "flex flex-wrap items-center gap-3 border-b px-4 py-3 text-ink",
        tone === "danger" ? "border-danger bg-danger-soft" : "border-warn bg-warn-soft",
      )}
    >
      <Icon aria-hidden className={cn("size-6 shrink-0", tone === "danger" ? "text-danger" : "text-warn")} />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <p className="font-bold">{title}</p>
        {children}
      </div>
      {action}
    </div>
  );
}

/**
 * Backup problems every signed-in user sees until they are solved, so a cashier can tell the
 * owner: the automatic backup failed, the newest backup is too old, or its second copy (stick or
 * folder) failed (warning banners); the database itself failed its check (a blocking alert to
 * acknowledge, then a red banner).
 */
export function BackupAlerts({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const navigate = useNavigate();
  const status = useBackupStatus();
  const [acknowledged, setAcknowledged] = useState(false);
  const data = status.data;
  if (!data) return null;
  const failed = data.lastAuto !== null && !data.lastAuto.ok;
  const openBackups = session.permissions.includes("settings.data") ? (
    <Button variant="outline" onClick={() => void navigate("/settings/backups")}>
      <DatabaseBackup aria-hidden />
      {t("common:backupAlerts.openBackups")}
    </Button>
  ) : null;
  return (
    <>
      {data.databaseDamaged ? (
        <Banner tone="danger" title={t("common:backupAlerts.damaged.title")} action={openBackups}>
          <p>{t("common:backupAlerts.damaged.body")}</p>
        </Banner>
      ) : null}
      {failed ? (
        <Banner tone="warn" title={t("common:backupAlerts.failed.title")} action={openBackups}>
          <p>{t("common:backupAlerts.failed.body")}</p>
          {data.lastAuto?.error ? (
            <p className="text-ink-2">{errorText({ code: data.lastAuto.error })}</p>
          ) : null}
        </Banner>
      ) : null}
      {!failed && data.stale && data.newestAt !== null ? (
        <Banner tone="warn" title={t("common:backupAlerts.stale.title")} action={openBackups}>
          <p>{t("common:backupAlerts.stale.body", { date: formatDate(data.newestAt, lang) })}</p>
        </Banner>
      ) : null}
      {data.mirrorError ? (
        <Banner tone="warn" title={t("common:backupAlerts.mirror.title")} action={openBackups}>
          <p>{t("common:backupAlerts.mirror.body")}</p>
          <p className="text-ink-2">{errorText({ code: data.mirrorError })}</p>
        </Banner>
      ) : null}
      {data.databaseDamaged ? (
        <Dialog
          open={!acknowledged}
          // Only the button closes it: Escape or a click outside must not hide this alert.
          onOpenChange={() => undefined}
          dismissible={false}
          title={t("common:backupAlerts.damaged.title")}
          description={t("common:backupAlerts.damaged.body")}
          footer={
            <Button variant="danger" onClick={() => setAcknowledged(true)}>
              {t("common:backupAlerts.damaged.acknowledge")}
            </Button>
          }
        >
          {null}
        </Dialog>
      ) : null}
    </>
  );
}
