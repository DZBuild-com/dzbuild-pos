import { useQueryClient } from "@tanstack/react-query";
import { Download, ExternalLink, RefreshCw, Sparkles } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { RadioCards } from "@/components/radio-cards";
import { Splash } from "@/components/splash";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { hasReleaseNotes } from "@/features/updates/release-notes";
import { ltrIn } from "@/features/updates/ready";
import { WhatsNewDialog } from "@/features/updates/WhatsNewDialog";
import { formatDate } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import { useOpenLink } from "@/lib/ipc/app";
import { toAppError } from "@/lib/ipc/client";
import { qk } from "@/lib/ipc/keys";
import { useTillLink } from "@/lib/ipc/lan";
import { type SettingKey, useSetSetting, useSetting } from "@/lib/ipc/settings";
import type {
  InstallBlocker,
  JsonValue,
  Session,
  UpdateChannel,
  UpdateOffer,
  UpdateProgress,
  UpdateStatus,
} from "@/lib/ipc/types";
import {
  useCheckUpdates,
  useDownloadUpdate,
  useInstallUpdate,
  useUpdateProgress,
  useUpdateStatus,
} from "@/lib/ipc/updates";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** Spec §13 Updates screen (PLT-010/011/012): the installed version, the channel, the automatic
 * checks and downloads, a check now, then the offered release with its notes and what can be done
 * with it — never an installation while a drawer is open. */
export function UpdatesSettings({ session }: { session: Session }) {
  const holds = session.permissions.includes("settings.data");
  const status = useUpdateStatus(holds);
  const errorText = useErrorText();
  if (status.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(status.error)}
      </p>
    );
  }
  if (!status.data) return <Splash inline />;
  return (
    <div className="flex flex-col gap-6">
      <VersionCard status={status.data} />
      <OfferCard status={status.data} />
    </div>
  );
}

function VersionCard({ status }: { status: UpdateStatus }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const client = useQueryClient();
  const check = useCheckUpdates();
  const set = useSetSetting();
  const channel = useSetting("updates.channel");
  const autoCheck = useSetting("updates.auto_check");
  const autoDownload = useSetting("updates.auto_download");
  const [whatsNew, setWhatsNew] = useState(false);
  // The channel and the automatic switches are the main PC's; read-only on a till.
  const till = useTillLink();
  // The status reads these settings: a change shows at once (the channel's next check uses it).
  const change = (key: SettingKey, value: JsonValue) =>
    set.mutate({ key, value }, { onSuccess: () => void client.invalidateQueries({ queryKey: qk.updates }) });
  // The status records each check's outcome; a check refused before it ran (one already running,
  // an invalid address) is only this button's error, shown while no later check answered.
  const refused =
    check.error && (status.lastCheckAt ?? 0) < check.submittedAt ? toAppError(check.error).code : null;
  const checkError = status.lastCheckError ?? refused;
  return (
    <Card aria-labelledby="updates-title">
      <h2 id="updates-title" className="text-xl font-bold">
        {t("settings:updates.title")}
      </h2>
      <dl className="grid grid-cols-[max-content_1fr] items-center gap-x-6 gap-y-2">
        <dt className="font-semibold">{t("settings:updates.installed")}</dt>
        <dd className="flex flex-wrap items-center gap-3">
          <bdi dir="ltr" className="text-lg font-bold">
            {status.currentVersion}
          </bdi>
          {hasReleaseNotes(status.currentVersion) ? (
            <Button variant="ghost" onClick={() => setWhatsNew(true)}>
              <Sparkles aria-hidden />
              {t("settings:updates.whatsNew")}
            </Button>
          ) : null}
        </dd>
      </dl>
      <HostOnlyNote />
      <fieldset disabled={till.isTill} className="contents">
        <RadioCards<UpdateChannel>
          legend={t("settings:updates.channel")}
          name="update-channel"
          value={channel.data === "beta" ? "beta" : "stable"}
          onChange={(value) => change("updates.channel", value)}
          options={[
            { value: "stable", label: t("settings:updates.stable"), hint: t("settings:updates.stableHint") },
            { value: "beta", label: t("settings:updates.beta"), hint: t("settings:updates.betaHint") },
          ]}
        />
        <Switch
          checked={autoCheck.data !== false}
          label={t("settings:updates.autoCheck")}
          onChange={(checked) => change("updates.auto_check", checked)}
        />
        <Switch
          checked={autoDownload.data !== false}
          label={t("settings:updates.autoDownload")}
          onChange={(checked) => change("updates.auto_download", checked)}
        />
      </fieldset>
      {set.error ? (
        <p role="alert" className="text-danger">
          {errorText(set.error)}
        </p>
      ) : null}
      <div className="flex flex-wrap items-center gap-3">
        <Button
          variant="outline"
          disabled={check.isPending || status.download.kind === "downloading"}
          onClick={() => check.mutate()}
        >
          <RefreshCw aria-hidden />
          {t("settings:updates.checkNow")}
        </Button>
        {checkError ? (
          <p role="alert" className="text-danger">
            {errorText({ code: checkError })}
          </p>
        ) : (
          <p className="text-ink-2">
            {status.lastCheckAt === null
              ? t("settings:updates.neverChecked")
              : t("settings:updates.lastCheck", {
                  date: ltrIn(formatDate(status.lastCheckAt, lang, { withTime: true }), lang),
                })}
          </p>
        )}
      </div>
      {whatsNew ? (
        <WhatsNewDialog version={status.currentVersion} onClose={() => setWhatsNew(false)} />
      ) : null}
    </Card>
  );
}

function Switch({
  checked,
  label,
  onChange,
}: {
  checked: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex min-h-11 items-center gap-3">
      <input
        type="checkbox"
        role="switch"
        className="size-6 accent-brand"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span>{label}</span>
    </label>
  );
}

/** The notes in the UI language, French when that language's are empty. */
function notesIn(offer: UpdateOffer, lang: Lang): string | null {
  if (!offer.notes) return null;
  return offer.notes[lang].trim() !== "" ? offer.notes[lang] : offer.notes.fr;
}

function OfferCard({ status }: { status: UpdateStatus }) {
  const { t } = useTranslation();
  const lang = useLang();
  const offer = status.offer;
  if (!offer) {
    return status.lastCheckAt !== null && status.lastCheckError === null ? (
      <Card>
        <p className="font-semibold text-ok">{t("settings:updates.upToDate")}</p>
      </Card>
    ) : null;
  }
  const notes = notesIn(offer, lang);
  const published = offer.pubDate === null ? Number.NaN : Date.parse(offer.pubDate);
  return (
    <Card aria-labelledby="update-offer-title">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h2 id="update-offer-title" className="text-xl font-bold">
          {t("settings:updates.available", { version: ltrIn(offer.version, lang) })}
        </h2>
        {Number.isNaN(published) ? null : (
          <bdi dir="ltr" className="text-ink-2">
            {formatDate(published, lang)}
          </bdi>
        )}
      </div>
      {notes ? <p className="whitespace-pre-line">{notes}</p> : null}
      <Decision status={status} offer={offer} />
    </Card>
  );
}

function Decision({ status, offer }: { status: UpdateStatus; offer: UpdateOffer }) {
  const { t } = useTranslation();
  const lang = useLang();
  const openLink = useOpenLink();
  switch (offer.decision.kind) {
    case "paused":
      return <p className="text-ink-2">{t("settings:updates.paused")}</p>;
    case "notYet":
      return <p className="text-ink-2">{t("settings:updates.notYet")}</p>;
    case "tooOld":
      return (
        <div className="flex flex-col items-start gap-3">
          <p className="text-warn">{t("settings:updates.tooOld")}</p>
          <Button variant="outline" onClick={() => openLink.mutate("download_page")}>
            <ExternalLink aria-hidden />
            {t("settings:updates.fullInstaller")}
          </Button>
        </div>
      );
    case "mainPcFirst":
      // A till takes the version its main PC runs (M6 review), from the main PC when it can.
      return (
        <p className="text-ink-2">
          {offer.decision.version
            ? t("settings:updates.mainPcFirst", { version: ltrIn(offer.decision.version, lang) })
            : t("settings:updates.mainPcUnreachable")}
        </p>
      );
    case "offer":
      return <DownloadAndInstall status={status} version={offer.version} />;
  }
}

/** "Télécharger" → the progress → "Installer maintenant", which asks first and stays disabled
 * while a drawer is open (one line per open drawer). */
function DownloadAndInstall({ status, version }: { status: UpdateStatus; version: string }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const download = useDownloadUpdate();
  const install = useInstallUpdate();
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [confirming, setConfirming] = useState(false);
  const client = useQueryClient();
  const state = status.download;
  useUpdateProgress((next) => {
    setProgress(next);
    // `updates_download` may answer before its task started: the status then still says idle.
    if (state.kind !== "downloading") void client.invalidateQueries({ queryKey: qk.updates });
  });

  if (state.kind === "downloading") {
    const live = progress ?? { received: state.received, total: state.total };
    const percent =
      live.total !== null && live.total > 0
        ? Math.min(100, Math.floor((live.received * 100) / live.total))
        : null;
    // Without a size from the server the bar only says that it runs (no value).
    return (
      <div className="flex flex-col gap-1">
        <div
          role="progressbar"
          aria-label={t("settings:updates.downloading")}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent ?? undefined}
          className="h-3 overflow-hidden rounded-full bg-surface-2"
        >
          <div
            className={
              percent === null
                ? "h-full w-1/3 bg-brand motion-safe:animate-pulse"
                : "h-full bg-brand transition-[inline-size]"
            }
            style={percent === null ? undefined : { inlineSize: `${percent}%` }}
          />
        </div>
        <p className="text-sm text-ink-2">{t("settings:updates.downloading")}</p>
      </div>
    );
  }

  if (state.kind === "ready") {
    const blocked = status.blockers.length > 0;
    return (
      <div className="flex flex-col items-start gap-3">
        {status.blockers.map((blocker) => (
          <BlockerLine key={`${blocker.terminalCode}-${blocker.openedAt}`} blocker={blocker} />
        ))}
        <Button
          variant="primary"
          disabled={blocked || status.installing || install.isPending}
          onClick={() => setConfirming(true)}
        >
          <Download aria-hidden />
          {t("settings:updates.install")}
        </Button>
        {confirming ? (
          <Dialog
            open
            onOpenChange={(open) => (open || install.isPending ? undefined : setConfirming(false))}
            title={t("settings:updates.confirmTitle")}
            description={t("settings:updates.confirmBody", { version: ltrIn(version, lang) })}
            footer={
              <>
                <Button variant="outline" disabled={install.isPending} onClick={() => setConfirming(false)}>
                  {t("common:actions.cancel")}
                </Button>
                <Button
                  variant="primary"
                  disabled={install.isPending || install.isSuccess}
                  onClick={() => install.mutate()}
                >
                  {t("settings:updates.confirm")}
                </Button>
              </>
            }
          >
            <p className="text-ink-2">{t("settings:updates.rollback")}</p>
            {install.error ? (
              <p role="alert" className="text-danger">
                {errorText(install.error)}
              </p>
            ) : null}
          </Dialog>
        ) : null}
      </div>
    );
  }

  // Idle, or a failed download (nothing kept): download (again).
  const failed = state.kind === "failed" ? state.code : null;
  return (
    <div className="flex flex-col items-start gap-3">
      {failed ? (
        <p role="alert" className="text-danger">
          {errorText({ code: failed })}
        </p>
      ) : null}
      {download.error ? (
        <p role="alert" className="text-danger">
          {errorText(download.error)}
        </p>
      ) : null}
      <Button
        variant="primary"
        disabled={download.isPending}
        onClick={() => {
          setProgress(null);
          download.mutate();
        }}
      >
        <Download aria-hidden />
        {t("settings:updates.download")}
      </Button>
    </div>
  );
}

function BlockerLine({ blocker }: { blocker: InstallBlocker }) {
  const { t } = useTranslation();
  const lang = useLang();
  return (
    <p role="alert" className="rounded-control bg-warn-soft p-3 font-semibold text-warn">
      {t("settings:updates.blocked", {
        terminal: blocker.terminalCode,
        user: blocker.userName,
        time: ltrIn(formatDate(blocker.openedAt, lang, { withTime: true }), lang),
      })}
    </p>
  );
}
