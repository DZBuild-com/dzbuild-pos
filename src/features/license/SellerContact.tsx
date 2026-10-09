import { useTranslation } from "react-i18next";
import type { LicenseSeller } from "@/lib/ipc/types";

/** Who sells the licences (the bundled `license.toml`): numbers and address as text, left to right. */
export function SellerContact({ seller }: { seller: LicenseSeller }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-1">
      <p className="font-semibold">{t("license:seller.title", { seller: seller.seller })}</p>
      <ul className="flex flex-col gap-0.5 text-ink-2">
        {seller.whatsapp ? (
          <li>
            {t("license:seller.whatsapp")} <bdi dir="ltr">+{seller.whatsapp}</bdi>
          </li>
        ) : null}
        {seller.phone ? (
          <li>
            {t("license:seller.phone")} <bdi dir="ltr">{seller.phone}</bdi>
          </li>
        ) : null}
        <li>
          {t("license:seller.web")} <bdi dir="ltr">{seller.url}</bdi>
        </li>
      </ul>
    </div>
  );
}
