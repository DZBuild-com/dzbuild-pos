import { TriangleAlert, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { hasReleaseNotes } from "@/features/updates/release-notes";
import { WhatsNewDialog } from "@/features/updates/WhatsNewDialog";
import { formatDate } from "@/lib/format";
import { useOpenLink, useStartupError, useStartupNotices } from "@/lib/ipc/app";
import type { StartupNotice } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** A version or a date inside a sentence, isolated left-to-right (the text form of
 * `<bdi dir="ltr">`, for a value interpolated into a translation). */
function ltr(text: string): string {
  return `\u2066${text}\u2069`;
}

export function StartupNotices() {
  const { t } = useTranslation();
  const startup = useStartupError();
  const notices = useStartupNotices(startup.isSuccess && startup.data === null);
  const [dismissed, setDismissed] = useState(false);
  if (dismissed || !notices.data || notices.data.length === 0) return null;
  return (
    <div
      role="alert"
      className="fixed inset-x-0 top-0 z-50 flex items-start gap-3 border-b border-warn bg-warn-soft px-4 py-3 text-ink shadow-overlay"
    >
      <TriangleAlert aria-hidden className="size-6 shrink-0 text-warn" />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        {notices.data.map((notice, index) => (
          <Notice key={`${index}-${notice.kind}`} notice={notice} />
        ))}
      </div>
      <Button
        variant="ghost"
        size="icon"
        aria-label={t("common:actions.close")}
        title={t("common:actions.close")}
        onClick={() => setDismissed(true)}
      >
        <X aria-hidden />
      </Button>
    </div>
  );
}

function Notice({ notice }: { notice: StartupNotice }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const lang = useLang();
  const openLink = useOpenLink();
  const [whatsNew, setWhatsNew] = useState<string | null>(null);
  switch (notice.kind) {
    case "restoreFailed":
      return (
        <div className="flex flex-col gap-1">
          <p className="font-bold">{t("common:startup.restoreFailed.title")}</p>
          <p>{t("common:startup.restoreFailed.body")}</p>
          <p className="text-ink-2">{errorText({ code: notice.reason })}</p>
        </div>
      );
    case "numbersReissued":
      return (
        <div className="flex flex-col gap-1">
          <p className="font-bold">{t("common:startup.numbersReissued.title")}</p>
          <p>{t("common:startup.numbersReissued.body")}</p>
          <ul className="flex flex-col gap-0.5 font-mono text-sm">
            {notice.ranges.map((range) => (
              <li key={`${range.docType}-${range.series}-${range.from}`}>
                <bdi dir="ltr">{range.first}</bdi>
                {range.to > range.from ? (
                  <>
                    {" … "}
                    <bdi dir="ltr">{range.last}</bdi>
                  </>
                ) : null}
              </li>
            ))}
          </ul>
        </div>
      );
    case "lanFailed":
    case "leaveFailed":
      // Leaving and installing can both fail at one start; a failed leave keeps a till.
      return (
        <p>
          {t(notice.kind === "lanFailed" ? "sync:lan.startup.failed" : "sync:lan.startup.leaveFailed", {
            reason: errorText({ code: notice.reason }),
          })}
        </p>
      );
    case "loggingUnavailable":
      return (
        <div className="flex flex-col gap-1">
          <p className="font-bold">{t("common:startup.loggingUnavailable.title")}</p>
          <p>{t("common:startup.loggingUnavailable.body")}</p>
          {notice.fallbackDir === null ? (
            <p>{t("common:startup.loggingUnavailable.none")}</p>
          ) : (
            <p className="flex flex-col gap-1">
              <span>{t("common:startup.loggingUnavailable.fallback")}</span>
              <bdi dir="ltr" className="font-mono text-sm break-all">
                {notice.fallbackDir}
              </bdi>
            </p>
          )}
          <p className="text-ink-2">{errorText({ code: notice.reason })}</p>
        </div>
      );
    case "webviewOutdated":
      return (
        <div className="flex flex-col items-start gap-1">
          <p className="font-bold">{t("common:startup.webviewOutdated.title")}</p>
          <p>
            {t("common:startup.webviewOutdated.body", {
              version: ltr(notice.version),
              minimum: ltr(notice.minimum),
            })}
          </p>
          <Button variant="outline" onClick={() => openLink.mutate("webview2_download")}>
            {t("common:startup.webviewOutdated.action")}
          </Button>
        </div>
      );
    case "clockMovedBack":
      return (
        <div className="flex flex-col gap-1">
          <p className="font-bold">{t("common:startup.clockMovedBack.title")}</p>
          <p>
            {t("common:startup.clockMovedBack.body", {
              lastSeen: ltr(formatDate(notice.lastSeenAt, lang, { withTime: true })),
              now: ltr(formatDate(notice.now, lang, { withTime: true })),
            })}
          </p>
        </div>
      );
    case "appUpdated":
      return (
        <div className="flex flex-col items-start gap-1">
          <p className="font-bold">{t("common:startup.appUpdated.title")}</p>
          <p>{t("common:startup.appUpdated.body", { from: ltr(notice.from), to: ltr(notice.to) })}</p>
          {hasReleaseNotes(notice.to) ? (
            <Button variant="outline" onClick={() => setWhatsNew(notice.to)}>
              {t("common:startup.appUpdated.whatsNew")}
            </Button>
          ) : null}
          {whatsNew ? <WhatsNewDialog version={whatsNew} onClose={() => setWhatsNew(null)} /> : null}
        </div>
      );
    case "updateNotInstalled":
      return (
        <div className="flex flex-col gap-1">
          <p className="font-bold">{t("common:startup.updateNotInstalled.title")}</p>
          <p>{t("common:startup.updateNotInstalled.body", { version: ltr(notice.version) })}</p>
        </div>
      );
    case "datapackSkipped":
      return (
        <p>
          {t("common:startup.datapackSkipped", {
            file: ltr(notice.file),
            reason: errorText({ code: notice.code }),
          })}
        </p>
      );
  }
}
