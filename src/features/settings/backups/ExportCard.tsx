import { ArchiveRestore, DatabaseBackup, FileSpreadsheet } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { formatDate } from "@/lib/format";
import { useExportBackupFile, useImportBackupFile } from "@/lib/ipc/data-files";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** DAT-006 "Exporter mes données": the whole shop as one `.dzpos` file, a link to every table as
 * spreadsheets (`/settings/data`), and M2's import of a `.dzpos` file into the list below. */
export function ExportCard() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const exportFile = useExportBackupFile();
  const importFile = useImportBackupFile();
  return (
    <Card aria-labelledby="backup-export-title">
      <h2 id="backup-export-title" className="text-xl font-bold">
        {t("settings:backups.export.title")}
      </h2>
      <p className="text-ink-2">{t("settings:backups.export.body")}</p>
      <p className="text-ink-2">{t("settings:data.importBackupHelp")}</p>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="outline"
          disabled={exportFile.isPending}
          onClick={() =>
            exportFile.mutate(undefined, {
              // `null`: the save dialog was cancelled.
              onSuccess: (path) => {
                if (path)
                  toast.success(t("settings:backups.export.done"), {
                    description: <bdi dir="ltr">{path}</bdi>,
                  });
              },
              onError: (error) => toast.error(errorText(error)),
            })
          }
        >
          <DatabaseBackup aria-hidden />
          {t("settings:backups.export.action")}
        </Button>
        <Button
          variant="outline"
          disabled={importFile.isPending}
          onClick={() =>
            importFile.mutate(undefined, {
              onSuccess: (backup) => {
                if (backup)
                  toast.success(
                    t("settings:data.importBackupDone", {
                      date: formatDate(backup.at, lang, { withTime: true }),
                    }),
                  );
              },
              onError: (error) => toast.error(errorText(error)),
            })
          }
        >
          <ArchiveRestore aria-hidden />
          {t("settings:data.importBackup")}
        </Button>
        <Link
          to="/settings/data"
          className="flex min-h-11 items-center gap-2 rounded-control px-3 font-semibold text-info underline hover:bg-surface-2 focus-visible:outline-3 focus-visible:outline-focus"
        >
          <FileSpreadsheet aria-hidden className="size-5 shrink-0" />
          {t("settings:backups.export.allData")}
        </Link>
      </div>
    </Card>
  );
}
