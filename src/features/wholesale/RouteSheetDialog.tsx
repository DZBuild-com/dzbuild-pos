import { Printer } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { LanguageToggle } from "@/components/language-toggle";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { ScaledPage } from "@/features/documents/PreviewDialog";
import type { Lang } from "@/lib/i18n";
import { usePrintRouteSheet, useRouteSheet } from "@/lib/ipc/wholesale";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/**
 * DOC-030: the round's route sheet (bon de route) as it prints — the shop, the driver and his
 * plate, every stop with its customer, document and amount due, the signatures — in any of the
 * three languages (the UI's first), then "Imprimer le bon de route": M3's print host prints it on
 * this PC's A4 printer (with the demo mark in the demo shop).
 */
export function RouteSheetDialog({
  roundId,
  number,
  onClose,
}: {
  roundId: string;
  number: string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const uiLang = useLang();
  const [lang, setLang] = useState<Lang>(uiLang);
  const page = useRouteSheet(roundId, lang);
  const print = usePrintRouteSheet();
  return (
    <Dialog
      open
      size="xl"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("wholesale:rounds.sheetTitle", { number })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.close")}
          </Button>
          <Button
            variant="primary"
            disabled={print.isPending || !page.data}
            onClick={() =>
              print.mutate(
                { id: roundId, lang },
                {
                  onSuccess: () => toast.success(t("wholesale:rounds.sheetPrinted")),
                  onError: (error) => toast.error(errorText(error)),
                },
              )
            }
          >
            <Printer aria-hidden />
            {t("wholesale:rounds.routeSheet")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-1.5">
        <span className="font-semibold">{t("wholesale:rounds.sheetLang")}</span>
        <LanguageToggle value={lang} onChange={setLang} />
      </div>
      {page.data ? (
        <ScaledPage page={page.data} />
      ) : page.error ? (
        <p role="alert" className="text-danger">
          {errorText(page.error)}
        </p>
      ) : (
        <p className="text-ink-2">{t("common:status.loading")}</p>
      )}
    </Dialog>
  );
}
