import { ArchiveRestore, DatabaseBackup, FileArchive, FileUp } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { importableEntities } from "@/features/imports/import-entities";
import { useCan } from "@/features/stock/stock-parts";
import { formatDate, formatNumber } from "@/lib/format";
import { useExportAllData, useExportBackupFile, useImportBackupFile } from "@/lib/ipc/data-files";
import type { BackupInfo } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/**
 * DAT-006: the whole database as a `.dzpos` file (a fresh backup, for a USB stick) and a `.dzpos`
 * file placed in the backups folder, where the restore list shows it. The dialogs are Rust's.
 */
export function BackupFileActions({ onImported }: { onImported?: (backup: BackupInfo) => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const exportFile = useExportBackupFile();
  const importFile = useImportBackupFile();
  return (
    <div className="flex flex-wrap gap-2">
      <Button
        variant="outline"
        disabled={exportFile.isPending}
        onClick={() =>
          exportFile.mutate(undefined, {
            // `null`: the save dialog was cancelled.
            onSuccess: (path) => {
              if (path) toast.success(t("settings:data.exportBackupDone"));
            },
            onError: (error) => toast.error(errorText(error)),
          })
        }
      >
        <DatabaseBackup aria-hidden />
        {t("settings:data.exportBackup")}
      </Button>
      <Button
        variant="outline"
        disabled={importFile.isPending}
        onClick={() =>
          importFile.mutate(undefined, {
            onSuccess: (backup) => {
              if (!backup) return;
              toast.success(
                t("settings:data.importBackupDone", {
                  date: formatDate(backup.at, lang, { withTime: true }),
                }),
              );
              onImported?.(backup);
            },
            onError: (error) => toast.error(errorText(error)),
          })
        }
      >
        <ArchiveRestore aria-hidden />
        {t("settings:data.importBackup")}
      </Button>
    </div>
  );
}

/**
 * `/settings/data` (IMX-015, DAT-006; `settings.data`): every table exported in open formats, the
 * backup files, and the imports the user may run.
 */
export function DataSettings() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const navigate = useNavigate();
  const can = useCan();
  const exportAll = useExportAllData();
  return (
    <div className="flex flex-col gap-6">
      <Card>
        <h2 className="text-xl font-bold">{t("settings:data.exportAllTitle")}</h2>
        <p className="text-ink-2">{t("settings:data.exportAllHelp")}</p>
        <div>
          <Button
            variant="primary"
            disabled={exportAll.isPending}
            onClick={() =>
              exportAll.mutate(undefined, {
                onSuccess: (done) => {
                  if (done)
                    toast.success(t("settings:data.exportAllDone", { rows: formatNumber(done.rows, lang) }));
                },
                onError: (error) => toast.error(errorText(error)),
              })
            }
          >
            <FileArchive aria-hidden />
            {t("settings:data.exportAll")}
          </Button>
        </div>
      </Card>
      <Card>
        <h2 className="text-xl font-bold">{t("settings:data.backupTitle")}</h2>
        <p className="text-ink-2">{t("settings:data.backupHelp")}</p>
        <p className="text-ink-2">{t("settings:data.importBackupHelp")}</p>
        <BackupFileActions onImported={() => void navigate("/settings/backups")} />
      </Card>
      <Card>
        <h2 className="text-xl font-bold">{t("settings:data.imports")}</h2>
        <p className="text-ink-2">{t("settings:data.importsHelp")}</p>
        <ul className="grid gap-2 sm:grid-cols-2">
          {importableEntities(can).map((entity) => (
            <li key={entity}>
              <Link
                to={`/imports/${entity}`}
                className="flex min-h-11 items-center gap-2 rounded-control border border-line px-3 font-semibold hover:bg-surface-2 focus-visible:outline-3 focus-visible:outline-focus"
              >
                <FileUp aria-hidden className="size-5 shrink-0 text-ink-2" />
                {t(`imports:entities.${entity}`)}
              </Link>
            </li>
          ))}
        </ul>
      </Card>
    </div>
  );
}
