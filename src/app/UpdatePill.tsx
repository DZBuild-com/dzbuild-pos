import { Download } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { useUpdateStatus } from "@/lib/ipc/updates";
import type { Session } from "@/lib/ipc/types";

/** Top bar: "Mise à jour prête" once an update is downloaded and verified, for whoever may install
 * it (`settings.data`); it opens the Updates screen. */
export function UpdatePill({ session }: { session: Session }) {
  const { t } = useTranslation();
  const holds = session.permissions.includes("settings.data");
  const status = useUpdateStatus(holds);
  if (!holds || status.data?.download.kind !== "ready") return null;
  return (
    <Link
      to="/settings/updates"
      className="inline-flex min-h-11 items-center gap-2 rounded-full bg-info-soft px-3 text-sm font-semibold text-info"
    >
      <Download aria-hidden className="size-4" />
      {t("common:updatePill")}
    </Link>
  );
}
