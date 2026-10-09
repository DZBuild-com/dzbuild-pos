import { FileUp, RotateCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { dayToMs, isDay } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useDatapack } from "@/lib/ipc/app";
import { useInstallPack, useInstalledPacks, useRestartApp } from "@/lib/ipc/privacy";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** Settings › Données réglementaires (PLT-024): the regulatory pack this start uses, the signed
 * packs installed on this PC, and "Installer un pack signé…": the pack's `.json` then its `.sig`
 * (Rust's file dialogs), verified against DZBuild's key, applied at the next start. */
export function RegulatorySettings() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const pack = useDatapack();
  const installed = useInstalledPacks();
  const install = useInstallPack();
  const restart = useRestartApp();
  const day = (value: string) => (
    <bdi dir="ltr">{isDay(value) ? formatDate(dayToMs(value), lang) : value}</bdi>
  );
  const notes = pack.data?.notes ?? [];
  const rows = installed.data ?? [];
  return (
    <div className="flex flex-col gap-6">
      <Card aria-labelledby="regulatory-title">
        <h2 id="regulatory-title" className="text-xl font-bold">
          {t("config:regulatory.title")}
        </h2>
        <p className="text-ink-2">{t("config:regulatory.intro")}</p>
        {pack.error ? (
          <p role="alert" className="text-danger">
            {errorText(pack.error)}
          </p>
        ) : pack.data ? (
          <div className="flex flex-col gap-3">
            <h3 className="font-bold">{t("config:regulatory.inUse")}</h3>
            <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1">
              <dt className="text-ink-2">{t("config:regulatory.id")}</dt>
              <dd>
                <bdi dir="ltr">{pack.data.id}</bdi>
              </dd>
              <dt className="text-ink-2">{t("config:regulatory.version")}</dt>
              <dd className="tabular-nums">{pack.data.version}</dd>
              <dt className="text-ink-2">{t("config:regulatory.validFrom")}</dt>
              <dd>{day(pack.data.validFrom)}</dd>
            </dl>
            {notes.length > 0 ? (
              <>
                <h4 className="font-semibold">{t("config:regulatory.notes")}</h4>
                {/* the pack's own source notes, as DZBuild wrote them (French) */}
                <ul className="list-disc ps-6 text-sm text-ink-2" lang="fr" dir="ltr">
                  {notes.map((note) => (
                    <li key={note}>{note}</li>
                  ))}
                </ul>
              </>
            ) : null}
          </div>
        ) : (
          <p className="text-ink-2">{t("common:status.loading")}</p>
        )}
      </Card>
      <Card aria-labelledby="regulatory-installed-title">
        <h2 id="regulatory-installed-title" className="text-xl font-bold">
          {t("config:regulatory.installed")}
        </h2>
        {installed.error ? (
          <p role="alert" className="text-danger">
            {errorText(installed.error)}
          </p>
        ) : rows.length === 0 ? (
          <p className="text-ink-2">
            {installed.isPending ? t("common:status.loading") : t("config:regulatory.installedEmpty")}
          </p>
        ) : (
          <table className="w-full border-collapse">
            <thead className="text-sm text-ink-2">
              <tr className="h-10">
                <th scope="col" className="text-start font-semibold">
                  {t("config:regulatory.id")}
                </th>
                <th scope="col" className="text-end font-semibold">
                  {t("config:regulatory.version")}
                </th>
                <th scope="col" className="text-start font-semibold">
                  {t("config:regulatory.validFrom")}
                </th>
                <th scope="col" className="text-start font-semibold">
                  {t("config:regulatory.file")}
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((p) => (
                <tr key={p.fileName} className="h-10 border-t border-line even:bg-surface-2">
                  <td>
                    <bdi dir="ltr">{p.id}</bdi>
                  </td>
                  <td className="text-end tabular-nums">{p.version}</td>
                  <td>{day(p.validFrom)}</td>
                  <td>
                    <bdi dir="ltr" className="text-sm">
                      {p.fileName}
                    </bdi>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        <div className="flex flex-col gap-2">
          <Button
            variant="primary"
            className="self-start"
            disabled={install.isPending}
            onClick={() => install.mutate()}
          >
            <FileUp aria-hidden />
            {t("config:regulatory.install")}
          </Button>
          <p className="text-sm text-ink-2">{t("config:regulatory.installHint")}</p>
        </div>
        {install.error ? (
          <p role="alert" className="text-danger">
            {errorText(install.error)}
          </p>
        ) : null}
        {install.data ? (
          <div className="flex flex-wrap items-center gap-3 rounded-control bg-ok-soft p-3">
            <p role="status" className="font-semibold text-ok">
              {t("config:regulatory.done")}
            </p>
            <Button
              variant="secondary"
              disabled={restart.isPending || restart.isSuccess}
              onClick={() => restart.mutate()}
            >
              <RotateCw aria-hidden />
              {restart.isSuccess ? t("config:regulatory.restarting") : t("config:regulatory.restart")}
            </Button>
          </div>
        ) : null}
        {restart.error ? (
          <p role="alert" className="text-danger">
            {errorText(restart.error)}
          </p>
        ) : null}
      </Card>
    </div>
  );
}
