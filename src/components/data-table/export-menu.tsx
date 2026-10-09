import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { NativeSelect } from "@/components/ui/native-select";
import { type Lang, LANGS, isLang } from "@/lib/i18n";
import { useExport } from "@/lib/ipc/exports";
import { useExportPdf } from "@/lib/ipc/print";
import type { ExportDone, ExportTarget, SheetFormat } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/**
 * Excel, CSV and PDF exports of a table, written by Rust after its own save dialog, in the file
 * language chosen here (the UI language by default, UX-043). The PDF is the same table laid out
 * by the print host (IMX-013), unless the screen passes its own `onPdf`.
 */
export function ExportMenu({ target, onPdf }: { target: ExportTarget | null; onPdf?: () => void }) {
  const { t } = useTranslation();
  const uiLang = useLang();
  const errorText = useErrorText();
  const [lang, setLang] = useState<Lang>(uiLang);
  const exporter = useExport();
  const pdf = useExportPdf();
  const handlers = {
    // `null`: the save dialog was cancelled.
    onSuccess: (done: ExportDone | null) => {
      if (done) toast.success(t("tables:export.done", { rows: done.rows }));
    },
    onError: (error: unknown) => toast.error(errorText(error)),
  };
  const run = (format: SheetFormat) => {
    if (!target) return;
    exporter.mutate({ target, format, lang }, handlers);
  };
  const runPdf = () => {
    if (onPdf) onPdf();
    else if (target) pdf.mutate({ target, lang }, handlers);
  };
  const busy = !target || exporter.isPending || pdf.isPending;
  return (
    <div className="flex flex-wrap items-center gap-2" role="group" aria-label={t("tables:export.title")}>
      <NativeSelect
        aria-label={t("tables:export.language")}
        className="w-auto"
        value={lang}
        onChange={(event) => {
          if (isLang(event.target.value)) setLang(event.target.value);
        }}
      >
        {LANGS.map((l) => (
          // Each language in its own script, whatever the UI language (like the language toggle).
          <option key={l} value={l} lang={l}>
            {t(`common:languages.${l}`)}
          </option>
        ))}
      </NativeSelect>
      <Button variant="secondary" disabled={busy} onClick={() => run("xlsx")}>
        {t("tables:export.xlsx")}
      </Button>
      <Button variant="secondary" disabled={busy} onClick={() => run("csv")}>
        {t("tables:export.csv")}
      </Button>
      <Button variant="secondary" disabled={busy && !onPdf} onClick={runPdf}>
        {t("tables:export.pdf")}
      </Button>
    </div>
  );
}
