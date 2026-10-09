import { Download } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { useInstallUpdate, useUpdateStatus } from "@/lib/ipc/updates";
import type { Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { installableVersion, ltrIn } from "./ready";

/** The sign-ins (`session.startedAt`) that already decided: once each, even when the shell
 * remounts (back from the till screen). */
const decided = new Set<number>();

/** PLT-011 "or next launch": an update downloaded before the sign-in, with no drawer open, is
 * offered once to whoever may install it (`settings.data`). The first status of the sign-in
 * decides (a login drops the cached one); an update ready later in the sign-in is offered after
 * the Z and by the top bar's pill, never by a dialog in the middle of the work. */
export function UpdateReadyDialog({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const holds = session.permissions.includes("settings.data");
  const status = useUpdateStatus(holds);
  const install = useInstallUpdate();
  const [atSignIn, setAtSignIn] = useState<{ version: string | null } | null>(() =>
    decided.has(session.startedAt) ? { version: null } : null,
  );
  const [closed, setClosed] = useState(false);
  if (atSignIn === null && status.data) setAtSignIn({ version: installableVersion(status.data) });
  useEffect(() => {
    if (atSignIn !== null) decided.add(session.startedAt);
  }, [atSignIn, session.startedAt]);
  const version = atSignIn?.version ?? null;
  if (version === null || closed) return null;
  return (
    <Dialog
      open
      onOpenChange={(next) => (next || install.isPending ? undefined : setClosed(true))}
      title={t("common:updatePill")}
      description={t("common:updateOffer.body", { version: ltrIn(version, lang) })}
      footer={
        <>
          <Button variant="outline" disabled={install.isPending} onClick={() => setClosed(true)}>
            {t("common:updateOffer.later")}
          </Button>
          <Button variant="primary" disabled={install.isPending} onClick={() => install.mutate()}>
            <Download aria-hidden />
            {t("common:updateOffer.install")}
          </Button>
        </>
      }
    >
      {install.error ? (
        <p role="alert" className="text-danger">
          {errorText(install.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
