import { CirclePlay, HardDriveDownload, Store } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { HelpPage } from "@/features/help/HelpPage";
import { useEnterDemo, useImportFirstRun } from "@/lib/ipc/demo";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { rememberSetupLang } from "./setup-lang";

/** The wizard's first screen, under its way on: try the demo shop (ONB-002), move the shop of
 * another PC from its `.dzpos` file (DAT-013; not for a till joining a shop), or watch the
 * 2-minute tour. The first two restart the app. */
export function FirstScreenOptions({ mode = "new" }: { mode?: "new" | "join" }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const enterDemo = useEnterDemo();
  const importShop = useImportFirstRun();
  const [topic, setTopic] = useState<string | null>(null);
  const openingDemo = enterDemo.isPending || enterDemo.isSuccess;
  const restoring = importShop.data === true;
  const busy = openingDemo || importShop.isPending || restoring;
  const error = enterDemo.error ?? importShop.error;
  return (
    <Card>
      <div className="flex flex-col gap-3">
        <Button
          size="lg"
          variant="outline"
          className="justify-start"
          disabled={busy}
          onClick={() => {
            importShop.reset();
            // Leaving the demo brings the wizard back in this language (finding 25).
            rememberSetupLang(lang);
            enterDemo.mutate(lang);
          }}
        >
          <Store aria-hidden />
          {t("setup:first.demo")}
        </Button>
        {mode === "join" ? null : (
          <div className="flex flex-col gap-1">
            <Button
              size="lg"
              variant="outline"
              className="justify-start"
              disabled={busy}
              onClick={() => {
                enterDemo.reset();
                importShop.mutate();
              }}
            >
              <HardDriveDownload aria-hidden />
              {t("setup:first.import")}
            </Button>
            {/* Finding 23: a moved shop keeps the old PC's terminal and ticket series. */}
            <p className="text-sm text-ink-2">{t("setup:first.importHint")}</p>
          </div>
        )}
        <Button size="lg" variant="outline" className="justify-start" onClick={() => setTopic("tour")}>
          <CirclePlay aria-hidden />
          {t("setup:first.tour")}
        </Button>
      </div>
      {openingDemo ? (
        <p role="status" className="font-semibold text-info">
          {t("setup:first.openingDemo")}
        </p>
      ) : null}
      {restoring ? (
        <p role="status" className="font-semibold text-info">
          {t("setup:first.restoring")}
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
          {errorText(error)}
        </p>
      ) : null}
      <Dialog
        open={topic !== null}
        onOpenChange={(open) => {
          if (!open) setTopic(null);
        }}
        title={t("setup:first.tour")}
        size="lg"
      >
        {topic !== null ? <HelpPage topic={topic} onTopic={setTopic} /> : null}
      </Dialog>
    </Card>
  );
}
