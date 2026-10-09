import { Copy } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { formatDate } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import { useConsents } from "@/lib/ipc/privacy";
import { useReadyMessage } from "@/lib/ipc/services";
import { useLang } from "@/lib/use-language";

/**
 * The "ready" text in the customer's language (`service_ready_message`), to copy into WhatsApp or
 * an SMS, with the customer's consent on each channel beside it (PDP-002): the POS never sends the
 * message itself, and without consent the cashier calls the customer instead.
 */
export function ReadyMessage({
  ticketId,
  customerId,
  lang,
}: {
  ticketId: string;
  customerId: string | null;
  lang: Lang;
}) {
  const { t } = useTranslation("services");
  const uiLang = useLang();
  const message = useReadyMessage(ticketId, lang);
  const consents = useConsents(customerId);
  const whatsapp = consents.data?.find((c) => c.channel === "whatsapp");
  const sms = consents.data?.find((c) => c.channel === "sms");
  const [copied, setCopied] = useState(false);
  const text = message.data;
  if (!text) return null;
  return (
    <section aria-label={t("ready.title")} className="flex flex-col gap-3">
      <h2 className="text-xl font-bold">{t("ready.title")}</h2>
      <p className="rounded-card bg-surface-2 p-4" lang={lang} dir={lang === "ar" ? "rtl" : "ltr"}>
        {text}
      </p>
      <div className="flex flex-wrap items-center gap-3">
        <Button
          variant="secondary"
          onClick={() => void navigator.clipboard.writeText(text).then(() => setCopied(true))}
        >
          <Copy aria-hidden />
          {t("ready.copy")}
        </Button>
        {copied ? <span role="status">{t("ready.copied")}</span> : null}
      </div>
      <p className={whatsapp?.granted ? "text-ok" : "text-warn"}>
        {whatsapp?.granted
          ? t("ready.whatsappYes", { date: formatDate(whatsapp.createdAt, uiLang) })
          : t("ready.whatsappNo")}
      </p>
      <p className={sms?.granted ? "text-ok" : "text-ink-2"}>
        {sms?.granted ? t("ready.smsYes", { date: formatDate(sms.createdAt, uiLang) }) : t("ready.smsNo")}
      </p>
      <p className="text-sm text-ink-2">{t("ready.hint")}</p>
    </section>
  );
}
