import { FileDown, Printer } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { LanguageToggle } from "@/components/language-toggle";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import type { Lang } from "@/lib/i18n";
import { useWarrantyLines } from "@/lib/ipc/billing";
import { toAppError } from "@/lib/ipc/client";
import { useDocument } from "@/lib/ipc/documents";
import { useExportWarrantyPdf, usePrintWarranty } from "@/lib/ipc/print";
import type { WarrantyInput } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/**
 * DOC-012 (décret 13-327 art. 6): the warranty certificate of one warranted line of a validated
 * ticket or invoice — its product, brand, serial numbers and duration as the backend found them
 * — printed on A4 or saved as a PDF in the language chosen here (the document's by default).
 */
export function WarrantyDialog({
  documentId,
  open,
  onClose,
}: {
  documentId: string;
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const uiLang = useLang();
  const errorText = useErrorText();
  const sources = useWarrantyLines(open ? documentId : null);
  const doc = useDocument(open ? documentId : null);
  const print = usePrintWarranty();
  const pdf = useExportWarrantyPdf();
  const [picked, setPicked] = useState<string | null>(null);
  const [chosenLang, setChosenLang] = useState<Lang | null>(null);
  if (!open) return null;

  const lines = sources.data ?? [];
  const lineId = picked ?? lines[0]?.lineId ?? null;
  const lang = chosenLang ?? doc.data?.lang ?? uiLang;
  const none =
    (sources.isSuccess && lines.length === 0) ||
    (sources.error != null && toAppError(sources.error).code === "warranty_not_set");
  const input: WarrantyInput | null = lineId ? { documentId, lineId, lang } : null;
  const failed = (error: unknown) => toast.error(errorText(error));
  const printNow = () => {
    if (!input) return;
    print.mutate(input, {
      onSuccess: (done) =>
        toast.success(t(done.dialog ? "billing:print.dialog" : "billing:warranty.printed")),
      onError: failed,
    });
  };
  const savePdf = () => {
    if (!input) return;
    pdf.mutate(input, {
      // `null`: the save dialog was cancelled.
      onSuccess: (saved) => {
        if (saved) toast.success(t("billing:print.pdfSaved", { path: saved.path }));
      },
      onError: failed,
    });
  };
  const busy = input === null || print.isPending || pdf.isPending;

  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("billing:warranty.title")}
      footer={
        <>
          <Button variant="secondary" disabled={busy} onClick={savePdf}>
            <FileDown aria-hidden />
            {t("billing:warranty.pdf")}
          </Button>
          <Button variant="primary" disabled={busy} onClick={printNow}>
            <Printer aria-hidden />
            {t("billing:warranty.print")}
          </Button>
        </>
      }
    >
      {none ? <p className="text-ink-2">{t("billing:warranty.none")}</p> : null}
      {sources.error && !none ? (
        <p role="alert" className="text-danger">
          {errorText(sources.error)}
        </p>
      ) : null}
      {lines.length > 0 ? (
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 font-semibold">{t("billing:warranty.line")}</legend>
          {lines.map((line) => (
            <label
              key={line.lineId}
              className={cn(
                "flex cursor-pointer items-start gap-3 rounded-card border-2 p-3",
                line.lineId === lineId ? "border-brand bg-brand-soft" : "border-line bg-surface",
              )}
            >
              <input
                type="radio"
                name="warranty-line"
                value={line.lineId}
                checked={line.lineId === lineId}
                onChange={() => setPicked(line.lineId)}
                className="mt-1 size-5 accent-brand"
              />
              <span className="flex min-w-0 flex-col gap-1">
                <span className="font-semibold">
                  {line.product}
                  {line.brand ? <span className="ms-2 font-normal text-ink-2">{line.brand}</span> : null}
                </span>
                <span>{t("billing:warranty.months", { months: line.months })}</span>
                {line.serials.length > 0 ? (
                  <span className="text-sm">
                    <span className="text-ink-2">{t("billing:warranty.serials")}</span>{" "}
                    <bdi dir="ltr">{line.serials.join(", ")}</bdi>
                  </span>
                ) : null}
              </span>
            </label>
          ))}
        </fieldset>
      ) : null}
      <div className="flex flex-col gap-1.5">
        <span className="font-semibold">{t("billing:warranty.language")}</span>
        <LanguageToggle value={lang} onChange={setChosenLang} />
      </div>
    </Dialog>
  );
}
