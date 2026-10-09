import { ShieldAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useCloudStatus } from "@/lib/ipc/cloud";
import { useCustomerFlag } from "@/lib/ipc/online-orders";

/** CRM-005 with DZBuild's list: a warning the cashier reads, never a block. Asked only while a
 * store is linked (DZB-015): an unlinked shop or a LAN till asks nothing. */
export function BuyerFlag({ phone }: { phone: string | null }) {
  const { t } = useTranslation();
  const linked = useCloudStatus().data?.linkStatus === "linked";
  const flag = useCustomerFlag(linked ? phone : null);
  if (!flag.data) return null;
  return (
    <div
      role="status"
      aria-label={t("sync:flag.title")}
      className="flex items-start gap-2 rounded-control bg-warn-soft p-3 text-warn"
    >
      <ShieldAlert aria-hidden className="size-5 shrink-0" />
      <div>
        <p className="font-semibold">{t("sync:flag.title")}</p>
        <p className="text-sm">{t(flag.data.isBanned ? "sync:flag.banned" : "sync:flag.risky")}</p>
      </div>
    </div>
  );
}
