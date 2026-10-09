import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { formatDate } from "@/lib/format";
import { useAcknowledgeNotice, useLanNotices } from "@/lib/ipc/lan";
import type { Notice } from "@/lib/ipc/types";
import { useLang } from "@/lib/use-language";

/** What a notice is about, for the owner: a product or document name the main PC put in
 * `details` (the `subject` itself is an id). */
export function noticeSubject(notice: Notice): string | null {
  const details = notice.details;
  if (typeof details !== "object" || details === null || Array.isArray(details)) return null;
  for (const key of ["productName", "documentNumber", "customerName", "zNumber"]) {
    const value = details[key];
    if (typeof value === "string" && value !== "") return value;
  }
  return null;
}

/** The title of a notice: its kind, or what a refused operation was refused for when that says
 * more (a ticket whose content no longer matches its own hash). */
export function noticeTitleKey(notice: Notice): string {
  const details = notice.details;
  const cause =
    typeof details === "object" && details !== null && !Array.isArray(details) ? details.cause : undefined;
  if (notice.kind === "replay_rejected" && cause === "replay_document_mismatch") {
    return "sync:lan.notices.kinds.document_mismatch";
  }
  return `sync:lan.notices.kinds.${notice.kind}`;
}

/** LAN-011: what the main PC noticed while taking the tills' operations (never deleted: read). */
export function NoticesPanel() {
  const { t } = useTranslation();
  const lang = useLang();
  const [showRead, setShowRead] = useState(false);
  const notices = useLanNotices(showRead);
  const acknowledge = useAcknowledgeNotice();
  const list = notices.data ?? [];
  return (
    <Card>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-xl font-bold">{t("sync:lan.notices.title")}</h2>
        <label className="flex min-h-11 items-center gap-2">
          <Checkbox checked={showRead} onCheckedChange={(value) => setShowRead(value === true)} />
          {t("sync:lan.notices.showRead")}
        </label>
      </div>
      {notices.isSuccess && list.length === 0 ? (
        <p className="text-ink-2">{t("sync:lan.notices.empty")}</p>
      ) : null}
      {list.length > 0 ? (
        <ul className="flex flex-col gap-2">
          {list.map((notice) => {
            const subject = noticeSubject(notice);
            return (
              <li
                key={notice.id}
                className="flex flex-wrap items-center gap-3 rounded-card border border-line p-3"
              >
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="font-semibold">{t(noticeTitleKey(notice))}</span>
                  {subject ? <bdi className="truncate">{subject}</bdi> : null}
                  <span className="text-sm text-ink-2">
                    {notice.terminalCode ? (
                      <>
                        <span>{t("sync:lan.notices.till", { code: notice.terminalCode })}</span>
                        {" · "}
                      </>
                    ) : null}
                    {formatDate(notice.createdAt, lang, { withTime: true })}
                  </span>
                </div>
                {notice.acknowledgedAt === null ? (
                  <Button
                    variant="outline"
                    disabled={acknowledge.isPending}
                    onClick={() => acknowledge.mutate(notice.id)}
                  >
                    {t("sync:lan.notices.acknowledge")}
                  </Button>
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : null}
    </Card>
  );
}
