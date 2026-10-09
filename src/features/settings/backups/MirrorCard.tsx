import { FolderOpen, FolderX, Usb } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { formatDate, formatNumber } from "@/lib/format";
import {
  useBackupToDrive,
  useChooseMirror,
  useClearMirror,
  useDrives,
  useMirrorStatus,
} from "@/lib/ipc/backup";
import { useDemoStatus } from "@/lib/ipc/demo";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const GIGABYTE = 1_073_741_824;

/** Stands for the drive letter while the sentence is split around it; never rendered. */
const MOUNT_SLOT = "\uE000";

/** "Copier sur {{drive}} ({{mount}})" with the drive letter in its own left-to-right island, so
 * `F:\` keeps its shape inside an Arabic sentence. */
function ToDriveLabel({ drive, mount }: { drive: string; mount: string }) {
  const { t } = useTranslation();
  const [before, after = ""] = t("settings:backups.mirror.toDrive.action", {
    drive,
    mount: MOUNT_SLOT,
  }).split(MOUNT_SLOT);
  return (
    <span>
      {before}
      <bdi dir="ltr">{mount}</bdi>
      {after}
    </span>
  );
}

/** DAT-008: a fresh backup copied to a stick plugged in now; the list follows the sticks within 3 s. */
function ToDrive() {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const drives = useDrives(true);
  const toDrive = useBackupToDrive();
  return (
    <div className="flex flex-col gap-2 border-t border-line pt-4">
      <h3 className="text-lg font-bold">{t("settings:backups.mirror.toDrive.title")}</h3>
      {drives.data && drives.data.length > 0 ? (
        <ul className="flex flex-wrap gap-2">
          {drives.data.map((d) => {
            const drive = d.label || d.mount;
            return (
              <li key={d.mount}>
                <Button
                  variant="outline"
                  disabled={toDrive.isPending}
                  onClick={() =>
                    toDrive.mutate(d.mount, {
                      onSuccess: () => toast.success(t("settings:backups.mirror.toDrive.done", { drive })),
                      onError: (error) => toast.error(errorText(error)),
                    })
                  }
                >
                  <Usb aria-hidden />
                  <ToDriveLabel drive={drive} mount={d.mount} />
                </Button>
              </li>
            );
          })}
        </ul>
      ) : drives.data ? (
        <p className="text-ink-2">{t("settings:backups.mirror.toDrive.none")}</p>
      ) : null}
      {drives.error ? (
        <p role="alert" className="text-danger">
          {errorText(drives.error)}
        </p>
      ) : null}
    </div>
  );
}

/** DAT-010/011: the second copy of every backup on a stick or a folder of this PC, its state, and
 * the sticks to copy to right now. */
export function MirrorCard() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const status = useMirrorStatus();
  const choose = useChooseMirror();
  const clear = useClearMirror();
  const mirror = status.data;
  const chosen = mirror?.folder != null;
  const onError = (error: unknown) => toast.error(errorText(error));
  // The demo shop's copies stay in its own folder, away from the real shop's stick.
  const demo = useDemoStatus().data?.active === true;
  // The card shows up with its state (one local call away), never as an empty frame first.
  if (status.isPending) return null;
  if (demo) {
    return (
      <Card aria-labelledby="backup-mirror-title">
        <h2 id="backup-mirror-title" className="text-xl font-bold">
          {t("settings:backups.mirror.title")}
        </h2>
        <p role="note" className="rounded-control bg-info-soft p-3">
          {t("settings:backups.mirror.demo")}
        </p>
      </Card>
    );
  }
  return (
    <Card aria-labelledby="backup-mirror-title">
      <h2 id="backup-mirror-title" className="text-xl font-bold">
        {t("settings:backups.mirror.title")}
      </h2>
      <p className="text-ink-2">{t("settings:backups.mirror.intro")}</p>
      {status.error ? (
        <p role="alert" className="text-danger">
          {errorText(status.error)}
        </p>
      ) : null}
      {mirror && mirror.folder !== null ? (
        <div className="flex flex-col gap-1">
          <p className="flex flex-wrap items-center gap-2">
            <bdi dir="ltr" className="font-semibold">
              {mirror.folder}
            </bdi>
            {mirror.label ? <bdi className="text-ink-2">{mirror.label}</bdi> : null}
            <Badge tone={mirror.available ? "ok" : "warn"}>
              {t(
                mirror.available
                  ? "settings:backups.mirror.available"
                  : "settings:backups.mirror.unavailable",
              )}
            </Badge>
          </p>
          {mirror.freeBytes !== null ? (
            <p className="text-ink-2">
              {t("settings:backups.mirror.free", {
                size: t("common:units.sizeGb", { value: formatNumber(mirror.freeBytes / GIGABYTE, lang, 1) }),
              })}
            </p>
          ) : null}
          <p>
            {mirror.lastCopyAt !== null
              ? t("settings:backups.mirror.lastCopy", {
                  date: formatDate(mirror.lastCopyAt, lang, { withTime: true }),
                })
              : t("settings:backups.mirror.never")}
          </p>
          {mirror.lastError ? <p className="text-danger">{errorText({ code: mirror.lastError })}</p> : null}
        </div>
      ) : mirror ? (
        <p>{t("settings:backups.mirror.none")}</p>
      ) : null}
      {mirror ? (
        <div className="flex flex-wrap gap-2">
          <Button
            variant={chosen ? "outline" : "primary"}
            disabled={choose.isPending}
            onClick={() => choose.mutate(undefined, { onError })}
          >
            <FolderOpen aria-hidden />
            {t(chosen ? "settings:backups.mirror.change" : "settings:backups.mirror.choose")}
          </Button>
          {chosen ? (
            <Button
              variant="ghost"
              disabled={clear.isPending}
              onClick={() => clear.mutate(undefined, { onError })}
            >
              <FolderX aria-hidden />
              {t("settings:backups.mirror.stop")}
            </Button>
          ) : null}
        </div>
      ) : null}
      <ToDrive />
    </Card>
  );
}
