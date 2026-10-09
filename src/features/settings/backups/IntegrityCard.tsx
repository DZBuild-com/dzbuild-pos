import { ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { useCheckDatabase } from "@/lib/ipc/backup";
import { useErrorText } from "@/lib/use-error-text";

/** The integrity check of the live database on demand; the result of the last click only. A damaged
 * database also raises the red banner every user sees (`BackupAlerts`). */
export function IntegrityCard() {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const check = useCheckDatabase();
  return (
    <Card aria-labelledby="backup-integrity-title">
      <h2 id="backup-integrity-title" className="text-xl font-bold">
        {t("settings:backups.integrity.title")}
      </h2>
      <p className="text-ink-2">{t("settings:backups.integrity.body")}</p>
      <div>
        <Button variant="outline" disabled={check.isPending} onClick={() => check.mutate()}>
          <ShieldCheck aria-hidden />
          {t("settings:backups.integrity.action")}
        </Button>
      </div>
      {check.data === true ? (
        <p role="status" className="font-semibold text-ok">
          {t("settings:backups.integrity.ok")}
        </p>
      ) : null}
      {check.data === false ? (
        <p role="alert" className="font-semibold text-danger">
          {t("settings:backups.integrity.damaged")}
        </p>
      ) : null}
      {check.error ? (
        <p role="alert" className="text-danger">
          {errorText(check.error)}
        </p>
      ) : null}
    </Card>
  );
}
