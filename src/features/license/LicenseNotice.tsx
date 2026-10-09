import { KeyRound } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import type { LicenseStatus } from "@/lib/ipc/types";
import { cn } from "@/lib/utils";
import { stateNoticeKey } from "./license-text";

/** The body of `LicenseBanner` (read-only mode and why, or a trial's last 3 days): a chunk of its
 * own read with the licence's texts only when there is something to say, out of the till's first
 * chunk (the /pos budget). */
export function LicenseNotice({ status, withLink }: { status: LicenseStatus; withLink: boolean }) {
  const { t } = useTranslation();
  const readOnly = !status.canWrite;
  const reason = stateNoticeKey(status);
  return (
    <div
      role="status"
      className={cn(
        "flex flex-wrap items-center gap-3 border-b px-4 py-2 text-ink",
        readOnly ? "border-danger bg-danger-soft" : "border-warn bg-warn-soft",
      )}
    >
      <KeyRound aria-hidden className={cn("size-5 shrink-0", readOnly ? "text-danger" : "text-warn")} />
      <div className="min-w-0 flex-1">
        {readOnly ? (
          <>
            <p className="font-semibold">{t("license:banner.readOnly")}</p>
            {reason ? <p>{t(reason, { code: status.previousDevice ?? "" })}</p> : null}
          </>
        ) : (
          <p className="font-semibold">{t("license:banner.trial", { count: status.daysLeft ?? 0 })}</p>
        )}
      </div>
      {withLink ? (
        <Link
          to="/settings/license"
          className="flex min-h-11 items-center rounded-control bg-surface px-4 font-semibold text-ink underline-offset-2 hover:underline focus-visible:outline-3 focus-visible:outline-focus"
        >
          {t(readOnly ? "license:banner.activate" : "license:banner.lifetime")}
        </Link>
      ) : null}
    </div>
  );
}
