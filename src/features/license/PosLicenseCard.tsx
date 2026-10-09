import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { backOfficeHome } from "@/app/nav";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import type { LicenseStatus, Session } from "@/lib/ipc/types";
import { stateNoticeKey } from "./license-text";

/** `PosLicenseGate`'s read-only card (why, and the way to the back office whose reports, exports and
 * backups still work): a chunk of its own read with the licence's texts only on a PC that cannot
 * sell (the /pos budget). */
export function PosLicenseCard({ session, status }: { session: Session; status: LicenseStatus }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const reason = stateNoticeKey(status);
  return (
    <main className="grid min-h-dvh place-items-center bg-bg p-6 text-ink">
      <Card className="max-w-xl">
        <h1 className="text-2xl font-bold">{t("license:till.title")}</h1>
        <p>{t("license:till.body")}</p>
        {reason ? <p className="text-ink-2">{t(reason, { code: status.previousDevice ?? "" })}</p> : null}
        {session.isOwner ? null : <p className="text-ink-2">{t("license:till.ask")}</p>}
        <div className="flex flex-wrap gap-3">
          <Button variant="primary" size="lg" onClick={() => void navigate(backOfficeHome(session))}>
            {t("license:till.backOffice")}
          </Button>
          {session.isOwner ? (
            <Button variant="outline" size="lg" onClick={() => void navigate("/settings/license")}>
              {t("license:banner.activate")}
            </Button>
          ) : null}
        </div>
      </Card>
    </main>
  );
}
