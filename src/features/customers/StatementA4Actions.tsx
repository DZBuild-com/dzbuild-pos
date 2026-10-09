import { FileDown, Printer } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { LanguageToggle } from "@/components/language-toggle";
import { Button } from "@/components/ui/button";
import { isDay } from "@/lib/dates";
import type { Lang } from "@/lib/i18n";
import { useExportStatementPdf, usePrintStatementA4 } from "@/lib/ipc/print";
import type { StatementSide } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/**
 * RPT-037, CRD-005: the A4 statement of a customer or a supplier over the period, printed on this
 * PC's A4 printer or saved as a PDF where the Rust save dialog says, in the language chosen here
 * — or in `lang` when the screen already has a statement language (M2's thermal statement).
 */
export function StatementA4Actions({
  side,
  partyId,
  from,
  to,
  lang: shared,
}: {
  side: StatementSide;
  partyId: string;
  from: string;
  to: string;
  lang?: Lang;
}) {
  const { t } = useTranslation();
  const uiLang = useLang();
  const errorText = useErrorText();
  const [own, setOwn] = useState<Lang>(uiLang);
  const print = usePrintStatementA4();
  const pdf = useExportStatementPdf();
  const lang = shared ?? own;
  const valid = isDay(from) && isDay(to) && from <= to;
  const input = { side, partyId, from, to, lang };
  const failed = (error: unknown) => toast.error(errorText(error));
  return (
    <div
      role="group"
      aria-label={t("customers:statement.title")}
      className="flex flex-wrap items-center gap-3"
    >
      {shared === undefined ? (
        <>
          <span className="font-semibold">{t("billing:statement.language")}</span>
          <LanguageToggle value={own} onChange={setOwn} />
        </>
      ) : null}
      <Button
        variant="secondary"
        disabled={!valid || print.isPending}
        onClick={() =>
          print.mutate(input, {
            onSuccess: (done) =>
              toast.success(t(done.dialog ? "billing:print.dialog" : "billing:statement.printed")),
            onError: failed,
          })
        }
      >
        <Printer aria-hidden />
        {t("billing:statement.printA4")}
      </Button>
      <Button
        variant="outline"
        disabled={!valid || pdf.isPending}
        onClick={() =>
          pdf.mutate(input, {
            // `null`: the save dialog was cancelled.
            onSuccess: (saved) => {
              if (saved) toast.success(t("billing:print.pdfSaved", { path: saved.path }));
            },
            onError: failed,
          })
        }
      >
        <FileDown aria-hidden />
        {t("billing:statement.pdf")}
      </Button>
    </div>
  );
}
