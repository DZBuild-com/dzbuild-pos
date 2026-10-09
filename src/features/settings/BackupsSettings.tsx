import { DatabaseBackup, Eye, FolderOpen } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { formatDate, formatNumber } from "@/lib/format";
import { useOpenFolder } from "@/lib/ipc/app";
import { useBackupNow, useBackups } from "@/lib/ipc/backup";
import type { BackupInfo, Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { ExportCard } from "./backups/ExportCard";
import { IntegrityCard } from "./backups/IntegrityCard";
import { MirrorCard } from "./backups/MirrorCard";
import { RestorePreviewDialog } from "./backups/RestorePreviewDialog";
import { ScheduleCard } from "./backups/ScheduleCard";

/** Spec §13 Backups screen: back up now, when and how long, the second copy and the sticks, the
 * integrity check, the export, then every backup with its preview before a restore. Backups stay
 * on this PC and the owner's own drives: there is no online backup (1.0.0, free edition). */
export function BackupsSettings({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const backups = useBackups();
  const now = useBackupNow();
  const open = useOpenFolder();
  const [previewing, setPreviewing] = useState<BackupInfo | null>(null);
  return (
    <div className="flex flex-col gap-6">
      <Card>
        <p className="text-ink-2">{t("settings:backups.intro")}</p>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="primary"
            disabled={now.isPending}
            onClick={() =>
              now.mutate(undefined, { onSuccess: () => toast.success(t("settings:backups.done")) })
            }
          >
            <DatabaseBackup aria-hidden />
            {t("settings:backups.now")}
          </Button>
          <Button variant="outline" onClick={() => open.mutate("backups")}>
            <FolderOpen aria-hidden />
            {t("settings:backups.openFolder")}
          </Button>
        </div>
        {now.error ? (
          <p role="alert" className="text-danger">
            {errorText(now.error)}
          </p>
        ) : null}
      </Card>
      <ScheduleCard />
      <MirrorCard />
      <IntegrityCard />
      <ExportCard />
      <Card>
        {backups.data && backups.data.length === 0 ? (
          <p className="text-ink-2">{t("settings:backups.empty")}</p>
        ) : (
          <table className="w-full border-collapse">
            <thead className="text-sm text-ink-2">
              <tr className="h-10">
                <th className="text-start font-semibold">{t("settings:backups.columns.date")}</th>
                <th className="text-start font-semibold">{t("settings:backups.columns.kind")}</th>
                <th className="text-end font-semibold">{t("settings:backups.columns.size")}</th>
                <th className="text-end font-semibold">{t("settings:backups.columns.actions")}</th>
              </tr>
            </thead>
            <tbody>
              {backups.data?.map((b) => (
                <tr key={b.fileName} className="h-12 border-t border-line even:bg-surface-2">
                  <td>
                    <bdi dir="ltr">{formatDate(b.at, lang, { withTime: true })}</bdi>
                  </td>
                  <td>{t(`settings:backups.kinds.${b.kind}`)}</td>
                  <td className="text-end">
                    <bdi dir="ltr">
                      {t("common:units.sizeMb", { value: formatNumber(b.size / 1_048_576, lang, 1) })}
                    </bdi>
                  </td>
                  <td className="text-end">
                    <Button variant="ghost" onClick={() => setPreviewing(b)}>
                      <Eye aria-hidden />
                      {t("settings:backups.previewAction")}
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
      {previewing ? (
        <RestorePreviewDialog backup={previewing} session={session} onClose={() => setPreviewing(null)} />
      ) : null}
    </div>
  );
}
