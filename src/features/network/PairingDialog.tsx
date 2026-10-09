import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { useCancelPairing, usePairingCode } from "@/lib/ipc/lan";
import { useErrorText } from "@/lib/use-error-text";

/** "042917" → "042 917": easier to read aloud and to type. */
export function groupCode(code: string): string {
  return `${code.slice(0, 3)} ${code.slice(3)}`;
}

/** m:ss of what is left, from this screen's clock (display only: the main PC decides). */
export function remaining(expiresAt: number, now: number): string {
  const seconds = Math.max(0, Math.ceil((expiresAt - now) / 1_000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

/** LAN-002: a single-use 6-digit code for 10 minutes, shown large; closing the dialog withdraws
 * it. The code is only displayed, never logged. */
export function PairingDialog({ addresses, onClose }: { addresses: string[]; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const issue = usePairingCode();
  const cancel = useCancelPairing();
  const [now, setNow] = useState(() => Date.now());
  const { mutate } = issue;
  // The countdown restarts from the moment the code arrives, not from the last tick.
  const newCode = useCallback(() => mutate(undefined, { onSuccess: () => setNow(Date.now()) }), [mutate]);

  useEffect(() => {
    newCode();
  }, [newCode]);
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, []);

  const close = () => {
    cancel.mutate();
    onClose();
  };
  const code = issue.data;
  const expired = code !== undefined && now >= code.expiresAt;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) close();
      }}
      title={t("sync:lan.pairing.title")}
      description={t("sync:lan.pairing.howTo")}
      // "Fermer" in the footer withdraws the code: no second close button in the title row
      dismissible={false}
      footer={
        <>
          {expired ? (
            <Button variant="outline" disabled={issue.isPending} onClick={newCode}>
              {t("sync:lan.pairing.newCode")}
            </Button>
          ) : null}
          <Button variant="primary" onClick={close}>
            {t("sync:lan.pairing.close")}
          </Button>
        </>
      }
    >
      {code && !expired ? (
        <div className="flex flex-col items-center gap-2">
          <span className="text-sm text-ink-2">{t("sync:lan.pairing.code")}</span>
          <bdi dir="ltr" className="font-mono text-5xl font-bold tracking-widest tabular-nums">
            {groupCode(code.code)}
          </bdi>
          <span className="text-ink-2">
            {t("sync:lan.pairing.expiresIn", { time: remaining(code.expiresAt, now) })}
          </span>
        </div>
      ) : null}
      {expired ? <p role="status">{t("sync:lan.pairing.expired")}</p> : null}
      {addresses.length > 0 ? (
        <div className="flex flex-col gap-1">
          <span className="text-sm text-ink-2">{t("sync:lan.pairing.address")}</span>
          {addresses.map((address) => (
            <bdi key={address} dir="ltr" className="font-mono text-lg">
              {address}
            </bdi>
          ))}
        </div>
      ) : null}
      {issue.error ? (
        <p role="alert" className="text-danger">
          {errorText(issue.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
