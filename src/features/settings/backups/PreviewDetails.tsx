import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { formatDate, formatNumber } from "@/lib/format";
import type { BackupPreview } from "@/lib/ipc/types";
import { useLang } from "@/lib/use-language";

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-wrap justify-between gap-x-4 border-b border-line py-2">
      <dt className="text-ink-2">{label}</dt>
      <dd className="font-semibold">{children}</dd>
    </div>
  );
}

/** What a sound backup holds: the shop, its last ticket, its counts and its last operation
 * (Settings › Backups and the fatal screen's guided restore). */
export function PreviewDetails({ preview }: { preview: BackupPreview }) {
  const { t } = useTranslation();
  const lang = useLang();
  const date = (ms: number) => <bdi dir="ltr">{formatDate(ms, lang, { withTime: true })}</bdi>;
  const origin = preview.origin;
  return (
    <dl className="flex flex-col">
      {origin?.demo ? (
        <Row label={t("settings:backups.preview.origin")}>
          <Badge tone="warn">{t("settings:backups.preview.demo")}</Badge>
        </Row>
      ) : null}
      {origin?.lanRole === "client" ? (
        <Row label={t("settings:backups.preview.origin")}>
          {t("settings:backups.preview.tillCopy", {
            code: origin.terminalCode ?? "",
            host: origin.hostName ?? "",
          })}
        </Row>
      ) : null}
      {origin?.lanRole === "host" && origin.activeTills > 0 ? (
        <Row label={t("settings:backups.preview.origin")}>
          {t("settings:backups.preview.mainCopy", { count: origin.activeTills })}
        </Row>
      ) : null}
      {preview.companyName ? (
        <Row label={t("settings:backups.preview.company")}>{preview.companyName}</Row>
      ) : null}
      <Row label={t("settings:backups.preview.lastTicket")}>
        {preview.lastTicket ? (
          <span className="flex flex-wrap gap-x-2">
            <bdi dir="ltr">{preview.lastTicket.number}</bdi>
            {date(preview.lastTicket.at)}
          </span>
        ) : (
          t("settings:backups.preview.noTicket")
        )}
      </Row>
      <Row label={t("settings:backups.preview.documents")}>{formatNumber(preview.documents, lang)}</Row>
      <Row label={t("settings:backups.preview.products")}>{formatNumber(preview.products, lang)}</Row>
      <Row label={t("settings:backups.preview.customers")}>{formatNumber(preview.customers, lang)}</Row>
      {preview.lastActivityAt !== null ? (
        <Row label={t("settings:backups.preview.lastActivity")}>{date(preview.lastActivityAt)}</Row>
      ) : null}
    </dl>
  );
}
