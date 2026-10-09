import "@/documents/documents.css";
import { useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { DocumentPage, pageRtl } from "@/documents/page";
import { usePrintModel } from "@/lib/ipc/print";
import type { HostPage, Paper, PrintOptions } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { cn } from "@/lib/utils";

/**
 * The page exactly as it prints (drafts marked BROUILLON), laid out by the print templates on
 * white paper and scaled down to the dialog's width; nothing is printed or journaled. With
 * `onPrint`, "Print…" hands over to the print dialog.
 */
export function PreviewDialog({
  id,
  options,
  open,
  onClose,
  onPrint,
}: {
  id: string;
  options: PrintOptions;
  open: boolean;
  onClose: () => void;
  onPrint?: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const model = usePrintModel(open ? { id, options } : null);
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("billing:preview.title")}
      size="xl"
      footer={
        onPrint ? (
          <Button variant="primary" onClick={onPrint}>
            {t("billing:preview.print")}
          </Button>
        ) : undefined
      }
    >
      {model.data ? (
        <ScaledPage page={model.data} />
      ) : model.error ? (
        <p role="alert" className="text-danger">
          {errorText(model.error)}
        </p>
      ) : (
        <p className="text-ink-2">{t("common:status.loading")}</p>
      )}
    </Dialog>
  );
}

/** A sheet of A4 paper (210 mm, 12 mm margins) or A5 (148 mm, 8 mm) scaled to fit the frame, in
 * the page's own direction: it shrinks from its top start corner. */
export function ScaledPage({ page, paper = "a4" }: { page: HostPage; paper?: Paper }) {
  const frame = useRef<HTMLDivElement>(null);
  const sheet = useRef<HTMLDivElement>(null);
  const [fit, setFit] = useState({ scale: 1, height: 0 });
  useLayoutEffect(() => {
    const measure = () => {
      const width = frame.current?.offsetWidth ?? 0;
      const natural = sheet.current?.offsetWidth ?? 0;
      const scale = width > 0 && natural > 0 ? Math.min(1, width / natural) : 1;
      setFit({ scale, height: (sheet.current?.offsetHeight ?? 0) * scale });
    };
    measure();
    const observer = new ResizeObserver(measure);
    if (frame.current) observer.observe(frame.current);
    return () => observer.disconnect();
  }, [page, paper]);
  const rtl = pageRtl(page);
  return (
    <div
      ref={frame}
      dir={rtl ? "rtl" : "ltr"}
      className="overflow-hidden rounded-control border border-line bg-surface-2"
      style={{ height: fit.height > 0 ? fit.height : undefined }}
    >
      <div
        ref={sheet}
        data-theme="light"
        className={cn("bg-white text-ink", paper === "a5" ? "w-[148mm] p-[8mm]" : "w-[210mm] p-[12mm]")}
        style={{ transform: `scale(${fit.scale})`, transformOrigin: rtl ? "top right" : "top left" }}
      >
        <DocumentPage page={page} />
      </div>
    </div>
  );
}
