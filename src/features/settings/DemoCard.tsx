import { Store, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { useDemoStatus, useEnterDemo, useEraseDemo } from "@/lib/ipc/demo";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** ONB-002 in Réglages › Diagnostic (owners): open the demo shop (the app restarts on it, the real
 * data stays as it is) or erase it. Hidden while the demo itself runs: its ribbon leads back. */
export function DemoCard() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const status = useDemoStatus();
  const enter = useEnterDemo();
  const erase = useEraseDemo();
  const [confirming, setConfirming] = useState(false);
  if (!status.data || status.data.active) return null;
  const opening = enter.isPending || enter.isSuccess;
  const error = enter.error ?? erase.error;
  return (
    <Card aria-labelledby="demo-card-title">
      <h2 id="demo-card-title" className="text-xl font-bold">
        {t("settings:demoCard.title")}
      </h2>
      <p className="text-ink-2">{t("settings:demoCard.body")}</p>
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" disabled={opening} onClick={() => setConfirming(true)}>
          <Store aria-hidden />
          {t("settings:demoCard.open")}
        </Button>
        <Button
          variant="outline"
          disabled={!status.data.exists || erase.isPending || opening}
          onClick={() => erase.mutate()}
        >
          <Trash2 aria-hidden />
          {t("settings:demoCard.erase")}
        </Button>
      </div>
      {opening ? (
        <p role="status" className="font-semibold text-info">
          {t("setup:first.openingDemo")}
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="text-danger">
          {errorText(error)}
        </p>
      ) : null}
      <Dialog
        open={confirming}
        onOpenChange={setConfirming}
        title={t("settings:demoCard.open")}
        footer={
          <>
            <Button variant="outline" onClick={() => setConfirming(false)}>
              {t("common:actions.cancel")}
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                setConfirming(false);
                erase.reset();
                enter.mutate(lang);
              }}
            >
              {t("settings:demoCard.open")}
            </Button>
          </>
        }
      >
        <p>{t("settings:demoCard.confirm")}</p>
      </Dialog>
    </Card>
  );
}
