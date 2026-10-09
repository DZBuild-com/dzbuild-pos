import { Download } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useInstallUpdate, useUpdateStatus } from "@/lib/ipc/updates";
import type { Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { ltrIn, offeredVersion } from "./ready";

/** PLT-011, in the close dialog once the Z is done: a downloaded update is offered to whoever may
 * install it (`settings.data`), now or later. */
export function UpdateAfterClose({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const holds = session.permissions.includes("settings.data");
  const status = useUpdateStatus(holds);
  const install = useInstallUpdate();
  const [later, setLater] = useState(false);
  const version = holds ? offeredVersion(status.data, !install.isIdle) : null;
  if (later || version === null) return null;
  return (
    <div className="flex flex-col gap-3 rounded-card border border-info bg-info-soft p-4 text-ink">
      <p className="font-semibold">{t("common:updateOffer.body", { version: ltrIn(version, lang) })}</p>
      {install.error ? (
        <p role="alert" className="text-danger">
          {errorText(install.error)}
        </p>
      ) : null}
      <div className="flex flex-wrap justify-end gap-2">
        <Button variant="outline" disabled={install.isPending} onClick={() => setLater(true)}>
          {t("common:updateOffer.later")}
        </Button>
        <Button variant="primary" disabled={install.isPending} onClick={() => install.mutate()}>
          <Download aria-hidden />
          {t("common:updateOffer.install")}
        </Button>
      </div>
    </div>
  );
}
