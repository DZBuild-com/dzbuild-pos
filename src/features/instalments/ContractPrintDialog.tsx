import { Printer } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { LanguageToggle } from "@/components/language-toggle";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { ScaledPage } from "@/features/documents/PreviewDialog";
import type { Lang } from "@/lib/i18n";
import { useContractPage, usePrintContract } from "@/lib/ipc/instalments";
import { useErrorText } from "@/lib/use-error-text";

/**
 * The instalment contract (TMP-001, TMP-009) as it prints, in the customer's language by default
 * (any of the three can be chosen), then "Imprimer le contrat": M3's print host prints the page on
 * this PC's A4 printer (with the demo mark in the demo shop).
 */
export function ContractPrintDialog({
  planId,
  lang: initial,
  onClose,
}: {
  planId: string;
  /** The customer's language. */
  lang: Lang;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const [lang, setLang] = useState<Lang>(initial);
  const page = useContractPage(planId, lang);
  const print = usePrintContract();
  return (
    <Dialog
      open
      size="xl"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("instalments:contract.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.close")}
          </Button>
          <Button
            variant="primary"
            disabled={print.isPending}
            onClick={() =>
              print.mutate(
                { planId, lang },
                {
                  onSuccess: () => toast.success(t("instalments:contract.printed")),
                  onError: (error) => toast.error(errorText(error)),
                },
              )
            }
          >
            <Printer aria-hidden />
            {t("instalments:wizard.printContract")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-1.5">
        <span className="font-semibold">{t("instalments:contract.lang")}</span>
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
