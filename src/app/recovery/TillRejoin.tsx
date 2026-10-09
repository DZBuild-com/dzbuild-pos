import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useRecoveryRejoin } from "@/lib/ipc/recovery";
import { useErrorText } from "@/lib/use-error-text";
import { DamagedFolder } from "./GuidedRestore";

/**
 * A till's data is a copy of its main PC's, so a backup would take it back behind the
 * main PC. Instead its damaged files are moved aside (never deleted) and the app restarts as a
 * new PC, whose setup joins the main PC again with a pairing code.
 */
export function TillRejoin({ onCancel, unsent = null }: { onCancel: () => void; unsent?: number | null }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const rejoin = useRecoveryRejoin();
  const section = useRef<HTMLElement>(null);
  useEffect(() => section.current?.focus(), [rejoin.isSuccess]);
  return (
    <section ref={section} tabIndex={-1} className="flex flex-col gap-4">
      {rejoin.data ? (
        <>
          <p role="status" className="font-semibold">
            {t("common:recovery.till.restarting")}
          </p>
          <DamagedFolder path={rejoin.data.damagedFolder} />
        </>
      ) : (
        <>
          <p>{t("common:recovery.till.explain")}</p>
          {/* M6 review: what the damaged copy had not sent stays with it, for support. */}
          <p className={unsent ? "font-semibold text-warn" : "text-ink-2"}>
            {unsent === null
              ? t("common:recovery.till.unsentUnknown")
              : unsent === 0
                ? t("common:recovery.till.unsentNone")
                : t("common:recovery.till.unsent", { count: unsent })}
          </p>
          {rejoin.error ? (
            <p role="alert" className="text-danger">
              {errorText(rejoin.error)}
            </p>
          ) : null}
          <div className="flex flex-wrap justify-end gap-3">
            <Button variant="outline" onClick={onCancel} disabled={rejoin.isPending}>
              {t("common:actions.cancel")}
            </Button>
            <Button variant="primary" onClick={() => rejoin.mutate()} disabled={rejoin.isPending}>
              {t("common:recovery.till.confirm")}
            </Button>
          </div>
        </>
      )}
    </section>
  );
}
