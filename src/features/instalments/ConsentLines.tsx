import { useTranslation } from "react-i18next";
import { formatDate } from "@/lib/format";
import { useConsents } from "@/lib/ipc/privacy";
import { useLang } from "@/lib/use-language";

/** The customer's latest answer on WhatsApp and SMS (PDP-002): the POS never sends a reminder
 * itself; without consent the cashier calls the customer. */
export function ConsentLines({ customerId }: { customerId: string | null }) {
  const { t } = useTranslation();
  const lang = useLang();
  const consents = useConsents(customerId);
  const whatsapp = consents.data?.find((c) => c.channel === "whatsapp");
  const sms = consents.data?.find((c) => c.channel === "sms");
  return (
    <div className="flex flex-col gap-1">
      <p className={whatsapp?.granted ? "text-ok" : "text-warn"}>
        {whatsapp?.granted
          ? t("instalments:consent.whatsappYes", { date: formatDate(whatsapp.createdAt, lang) })
          : t("instalments:consent.whatsappNo")}
      </p>
      <p className={sms?.granted ? "text-ok" : "text-ink-2"}>
        {sms?.granted
          ? t("instalments:consent.smsYes", { date: formatDate(sms.createdAt, lang) })
          : t("instalments:consent.smsNo")}
      </p>
    </div>
  );
}
