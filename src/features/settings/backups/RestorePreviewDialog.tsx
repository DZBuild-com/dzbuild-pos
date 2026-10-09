import { useState } from "react";
import { useTranslation } from "react-i18next";
import { PinInput } from "@/components/pin-pad";
import { Splash } from "@/components/splash";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { formatDate } from "@/lib/format";
import { useBackupPreview, useRestoreBackup } from "@/lib/ipc/backup";
import type { BackupInfo, Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { PreviewDetails } from "./PreviewDetails";

/**
 * "Aperçu et restauration" (DAT-010): what the backup holds, read without changing it, before
 * anything is replaced. Only a sound backup of this version or an older one can be restored, only
 * by the owner and with the owner's PIN; the app then restarts on it.
 */
export function RestorePreviewDialog({
  backup,
  session,
  onClose,
}: {
  backup: BackupInfo;
  session: Session;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const preview = useBackupPreview(backup.fileName);
  const restore = useRestoreBackup();
  const [pin, setPin] = useState("");
  const restarting = restore.isSuccess;
  const status = preview.data?.status;
  const canRestore = status === "valid" && session.isOwner;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !restarting) onClose();
      }}
      title={t("settings:backups.preview.title")}
      description={`${formatDate(backup.at, lang, { withTime: true })} · ${t(`settings:backups.kinds.${backup.kind}`)}`}
      footer={
        restarting ? null : (
          <>
            <Button variant="outline" onClick={onClose}>
              {t(canRestore ? "common:actions.cancel" : "common:actions.close")}
            </Button>
            {canRestore ? (
              <Button
                variant="primary"
                disabled={pin.length < 4 || restore.isPending}
                onClick={() => restore.mutate({ fileName: backup.fileName, pin })}
              >
                {t("settings:backups.restoreConfirm")}
              </Button>
            ) : null}
          </>
        )
      }
    >
      {restarting ? (
        <p role="status">{t("settings:backups.restarting")}</p>
      ) : preview.isPending ? (
        <Splash inline />
      ) : preview.error ? (
        <p role="alert" className="text-danger">
          {errorText(preview.error)}
        </p>
      ) : (
        <>
          {status === "valid" ? <PreviewDetails preview={preview.data} /> : null}
          {status === "invalid" ? (
            <p role="alert" className="text-danger">
              {t("settings:backups.preview.invalid")}
            </p>
          ) : null}
          {status === "newer_than_app" ? (
            <p role="alert" className="text-danger">
              {t("settings:backups.preview.newer")}
            </p>
          ) : null}
          {canRestore ? (
            <>
              <p className="text-ink-2">
                {t("settings:backups.restoreBody", { date: formatDate(backup.at, lang, { withTime: true }) })}
              </p>
              {/* Finding 23: a shop moved from another PC keeps that PC's terminal and series. */}
              <p className="text-ink-2">{t("settings:backups.restoreOtherPc")}</p>
              <Field id="restore-pin" label={t("settings:backups.restorePin")}>
                <PinInput id="restore-pin" value={pin} onChange={setPin} />
              </Field>
            </>
          ) : null}
          {status === "valid" && !session.isOwner ? (
            <p className="text-ink-2">{t("settings:backups.preview.ownerOnly")}</p>
          ) : null}
        </>
      )}
      {restore.error ? (
        <p role="alert" className="text-danger">
          {errorText(restore.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
