import { Download, FolderOpen, History, Network, ShieldAlert } from "lucide-react";
import { lazy, Suspense, useState } from "react";
import { useTranslation } from "react-i18next";
import { LanguageToggle } from "@/components/language-toggle";
import { Button } from "@/components/ui/button";
import { loadBackOffice, setLanguage } from "@/lib/i18n";
import { useOpenFolder, useOpenLink } from "@/lib/ipc/app";
import type { AppErrorPayload } from "@/lib/ipc/client";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { TillRejoin } from "./recovery/TillRejoin";

// The guided restore reads the backups' words, back-office texts (`settings`): its chunk and
// those texts load together when the owner asks for it (offline: both ship inside the app).
const GuidedRestore = lazy(() =>
  Promise.all([import("./recovery/GuidedRestore"), loadBackOffice()]).then(([m]) => ({
    default: m.GuidedRestore,
  })),
);

/** The failures a backup can repair: the database is damaged, missing, or a restore stopped. */
const RECOVERABLE = new Set([
  "database_corrupt",
  "database_missing",
  "restore_needs_attention",
  "database_error",
]);

/**
 * The failures a till repairs by setting its copy aside (Rust's `REJOINABLE`, which
 * `recovery_rejoin` checks too): the copy cannot be used. A `database_error` (a locked file, a
 * disk error, a failed migration) keeps M0's screen on a till: a retry may open the files, and
 * they may hold sales the main PC never received.
 */
const REJOINABLE = new Set(["database_corrupt", "database_missing", "restore_needs_attention"]);

/**
 * The database could not open: say what happened, where the data is, and keep the data safe.
 * A damaged or missing database offers the guided restore; on a till (`tillOf`) a copy
 * it cannot use offers to join the main PC again instead; a database from a newer version sends
 * the owner to the latest version.
 */
export function FatalScreen({
  failure,
}: {
  failure: {
    error: AppErrorPayload;
    dataDir: string;
    tillOf?: string | null;
    unsent?: number | null;
    mainPcTills?: number | null;
  };
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const open = useOpenFolder();
  const openLink = useOpenLink();
  const [mode, setMode] = useState<"info" | "restore" | "rejoin">("info");
  const code = failure.error.code;
  const till = failure.tillOf ?? null;
  const offersRestore = RECOVERABLE.has(code) && till === null;
  const offersRejoin = REJOINABLE.has(code) && till !== null;
  const newer = code === "database_newer_than_app";
  return (
    <main className="grid min-h-dvh place-items-center bg-bg p-6 text-ink">
      <div className="flex max-w-xl flex-col gap-4 rounded-card border border-danger bg-surface p-8">
        <ShieldAlert aria-hidden className="size-10 text-danger" />
        <h1 className="text-2xl font-bold">{t("common:fatal.title")}</h1>
        <p role="alert" className="text-danger">
          {errorText(failure.error)}
        </p>
        {mode === "restore" ? (
          <Suspense fallback={null}>
            <GuidedRestore onCancel={() => setMode("info")} allowFresh={code === "database_missing"} />
          </Suspense>
        ) : null}
        {mode === "rejoin" ? (
          <TillRejoin onCancel={() => setMode("info")} unsent={failure.unsent ?? null} />
        ) : null}
        {mode === "info" ? (
          <>
            <p>{t("common:fatal.body")}</p>
            {newer ? <p>{t("common:recovery.newerDatabase")}</p> : null}
            {offersRejoin ? (
              <>
                <p>{t("common:recovery.till.body")}</p>
                {till ? (
                  <p className="flex flex-col gap-1">
                    <span className="font-semibold">{t("common:recovery.till.host")}</span>
                    <bdi className="rounded-control bg-surface-2 p-2">{till}</bdi>
                  </p>
                ) : null}
              </>
            ) : null}
            {offersRestore && (failure.mainPcTills ?? 0) > 0 ? (
              <p role="note" className="rounded-control bg-info-soft p-3">
                {t("common:recovery.mainPc", { count: failure.mainPcTills ?? 0 })}
              </p>
            ) : null}
            {offersRestore ? (
              <Button variant="primary" onClick={() => setMode("restore")}>
                <History aria-hidden />
                {t("common:recovery.start")}
              </Button>
            ) : null}
            {offersRejoin ? (
              <Button variant="primary" onClick={() => setMode("rejoin")}>
                <Network aria-hidden />
                {t("common:recovery.till.start")}
              </Button>
            ) : null}
            {newer ? (
              <Button variant="primary" onClick={() => openLink.mutate("download_page")}>
                <Download aria-hidden />
                {t("common:recovery.download")}
              </Button>
            ) : null}
            {/* No data folder at all (Windows could not name it): nothing to show or open. */}
            {failure.dataDir ? (
              <>
                <p className="flex flex-col gap-1">
                  <span className="font-semibold">{t("common:fatal.dataFolder")}</span>
                  <bdi dir="ltr" className="rounded-control bg-surface-2 p-2 font-mono text-sm break-all">
                    {failure.dataDir}
                  </bdi>
                </p>
                <Button
                  variant={offersRestore || offersRejoin || newer ? "outline" : "primary"}
                  onClick={() => open.mutate("data")}
                >
                  <FolderOpen aria-hidden />
                  {t("common:fatal.openFolder")}
                </Button>
              </>
            ) : null}
          </>
        ) : null}
        <LanguageToggle value={lang} onChange={(next) => void setLanguage(next)} />
      </div>
    </main>
  );
}
