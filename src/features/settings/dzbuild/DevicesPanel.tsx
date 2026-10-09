import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { formatDate } from "@/lib/format";
import { useCloudDevices, useOpenCloudPage } from "@/lib/ipc/cloud";
import type { Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** The tills linked to the store, asked of DZBuild only when the owner opens the list. */
export function DevicesPanel({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [show, setShow] = useState(false);
  const devices = useCloudDevices(show);
  const open = useOpenCloudPage();
  return (
    <Card>
      <h3 className="text-lg font-semibold">{t("sync:devices.title")}</h3>
      <p className="text-sm text-ink-2">{t("sync:devices.help")}</p>
      {devices.error ? <p className="text-ink-2">{errorText(devices.error)}</p> : null}
      {devices.data && devices.data.length > 0 ? (
        <ul>
          {devices.data.map((d) => (
            <li key={d.id} className="flex min-h-11 items-center gap-3 border-t border-line">
              <span className="grow">
                {d.name ?? d.id}
                {d.thisTill ? (
                  <span className="ms-2 text-sm text-ok">{t("sync:devices.thisTill")}</span>
                ) : null}
              </span>
              <span className="text-sm text-ink-2">
                {d.lastSeenAt ? (
                  <bdi dir="ltr">{formatDate(d.lastSeenAt, lang, { withTime: true })}</bdi>
                ) : (
                  "—"
                )}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
      <div className="flex flex-wrap gap-2">
        {show ? null : (
          <Button variant="outline" onClick={() => setShow(true)}>
            {t("sync:devices.show")}
          </Button>
        )}
        <Button
          variant="ghost"
          disabled={!session.isOwner}
          title={session.isOwner ? undefined : t("sync:link.ownerOnly")}
          onClick={() => open.mutate("devices", { onError: (error) => toast.error(errorText(error)) })}
        >
          {t("sync:devices.manage")}
        </Button>
      </div>
    </Card>
  );
}
