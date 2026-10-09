import { FileDown, Printer } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { LabelSheet, sheetRtl } from "@/documents/label-sheet";
import { useHardwareConfig } from "@/lib/ipc/hardware";
import {
  useBuildLabels,
  useLabelSheet,
  useLabelTemplates,
  useSheetPdf,
  useSheetPrint,
} from "@/lib/ipc/labels";
import type { LabelLang, LabelRequest, SheetInput } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { cn } from "@/lib/utils";
import { labelErrorText, productsWith } from "./warnings";

/** The A4 page on screen: 210 × 297 mm at 40 %. */
const PREVIEW_ZOOM = 0.4;

/**
 * LBL-013: the labels of `request` on an A4 planche for a laser or inkjet printer. The user picks
 * the planche and the first free position of a partly used one (the used positions greyed, in the
 * sheet's direction: an Arabic sheet starts at its top right). Rust lays the sheet out (prices,
 * words, barcodes; the dialog computes none) and the print host prints it on this PC's A4 printer
 * or saves it as a PDF; Rust journals the labels once that succeeded. Food and regulated goods go
 * without Arabic only after the confirmation `PrintLabelsDialog` asks for (DOC-034).
 */
export function SheetDialog({ request, onClose }: { request: LabelRequest; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const templates = useLabelTemplates();
  const hardware = useHardwareConfig();
  const [formatId, setFormatId] = useState<string | null>(null);
  const [start, setStart] = useState(1);
  const [lang, setLang] = useState<LabelLang | null>(request.lang);
  const [confirming, setConfirming] = useState(false);
  const [arabicOverride, setArabicOverride] = useState(false);
  const formats = templates.data?.sheetFormats ?? [];
  const format = formats.find((f) => f.id === formatId) ?? formats[0];
  const labels: LabelRequest = { ...request, lang };
  const batch = useBuildLabels(labels);
  const warnings = batch.data?.warnings ?? [];
  const arabicRequired = productsWith(warnings, "arabic_required");
  const arabicNameMissing = productsWith(warnings, "arabic_name_missing");
  const guarded = arabicRequired + arabicNameMissing > 0 && !arabicOverride;
  const input: SheetInput | null =
    format && batch.data && !guarded ? { request: labels, formatId: format.id, start, arabicOverride } : null;
  const sheet = useLabelSheet(input);
  const print = useSheetPrint();
  const pdf = useSheetPdf();
  const printer = hardware.data?.documents?.defaultPrinter ?? null;
  const total = batch.data?.total ?? 0;
  const busy = input === null || !sheet.isSuccess || print.isPending || pdf.isPending;
  const choose = (change: () => void) => {
    change();
    print.reset();
    pdf.reset();
  };

  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("labels:sheet.title")}
      footer={
        <>
          <Button variant="secondary" disabled={busy} onClick={() => input && pdf.mutate(input)}>
            <FileDown aria-hidden />
            {t("labels:sheet.pdf")}
          </Button>
          <Button variant="primary" disabled={busy} onClick={() => input && print.mutate(input)}>
            <Printer aria-hidden />
            {t("labels:sheet.print")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 md:grid-cols-2">
        <div className="flex flex-col gap-3">
          <Field id="sheet-format" label={t("labels:sheet.format")}>
            <NativeSelect
              id="sheet-format"
              value={format?.id ?? ""}
              onChange={(e) =>
                choose(() => {
                  setFormatId(e.target.value);
                  setStart(1);
                })
              }
            >
              {formats.map((f) => (
                <option key={f.id} value={f.id}>
                  {t(`labels:sheet.formats.${f.id}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
          <p>
            <span className="font-semibold">{t("labels:sheet.printer")}</span>{" "}
            {printer ?? t("billing:print.defaultPrinter")}
          </p>
          {format && sheet.data ? (
            <fieldset className="flex flex-col gap-2">
              <legend className="mb-1 font-semibold">{t("labels:sheet.start")}</legend>
              <div
                dir={sheetRtl(sheet.data) ? "rtl" : "ltr"}
                className="grid gap-1 rounded-control border border-line bg-surface-2 p-2"
                style={{ gridTemplateColumns: `repeat(${format.columns}, minmax(0, 1fr))` }}
              >
                {Array.from({ length: format.columns * format.rows }, (_, i) => i + 1).map((n) => (
                  <button
                    key={n}
                    type="button"
                    aria-pressed={n === start}
                    onClick={() => choose(() => setStart(n))}
                    className={cn(
                      "h-11 rounded-control border font-semibold tabular-nums focus-visible:ring-3 focus-visible:ring-focus focus-visible:outline-none",
                      n < start
                        ? "border-line bg-surface-2 text-ink-2 line-through"
                        : n < start + total
                          ? "border-brand bg-brand-soft text-ink"
                          : "border-line bg-surface text-ink",
                      n === start && "border-2 border-ink",
                    )}
                  >
                    {n}
                  </button>
                ))}
              </div>
              <p className="text-ink-2">{t("labels:sheet.used")}</p>
            </fieldset>
          ) : null}
        </div>
        {sheet.data ? (
          <figure className="flex flex-col items-center gap-2">
            <div
              data-testid="sheet-preview"
              role="img"
              aria-label={t("labels:sheet.preview")}
              className="overflow-hidden border border-line"
              style={{ zoom: PREVIEW_ZOOM }}
            >
              <LabelSheet sheet={{ ...sheet.data, pages: Math.min(sheet.data.pages, 1) }} />
            </div>
            <figcaption>{t("labels:sheet.pages", { count: sheet.data.pages })}</figcaption>
          </figure>
        ) : null}
      </div>
      {guarded ? (
        <div
          role="alert"
          className="flex flex-col gap-3 rounded-card border border-warn bg-warn-soft p-4 text-warn"
        >
          {arabicRequired > 0 ? <p>{t("labels:print.arabicRequired", { count: arabicRequired })}</p> : null}
          {arabicNameMissing > 0 ? (
            <p>{t("labels:print.arabicNameMissing", { count: arabicNameMissing })}</p>
          ) : null}
          {confirming ? (
            <div className="flex flex-wrap gap-2">
              <Button
                variant="danger"
                onClick={() =>
                  choose(() => {
                    setArabicOverride(true);
                    setConfirming(false);
                  })
                }
              >
                {t("labels:print.confirmWithoutArabic")}
              </Button>
              <Button variant="ghost" onClick={() => setConfirming(false)}>
                {t("common:actions.cancel")}
              </Button>
            </div>
          ) : (
            <div className="flex flex-wrap gap-2">
              {arabicRequired > 0 ? (
                <Button variant="secondary" onClick={() => choose(() => setLang("ar_fr"))}>
                  {t("labels:print.switchToArabic")}
                </Button>
              ) : null}
              <Button variant="outline" onClick={() => setConfirming(true)}>
                {t("labels:print.withoutArabic")}
              </Button>
            </div>
          )}
        </div>
      ) : null}
      {print.isSuccess ? (
        <p role="status" className="font-semibold text-ok">
          {t(print.data.dialog ? "billing:print.dialog" : "labels:sheet.printed")}
        </p>
      ) : null}
      {pdf.data ? (
        <p role="status" className="font-semibold text-ok">
          {t("billing:print.pdfSaved", { path: pdf.data.path })}
        </p>
      ) : null}
      {print.error ? <p className="text-danger">{labelErrorText(print.error, t, errorText)}</p> : null}
      {pdf.error ? <p className="text-danger">{labelErrorText(pdf.error, t, errorText)}</p> : null}
      {sheet.error ? <p className="text-danger">{labelErrorText(sheet.error, t, errorText)}</p> : null}
      {batch.error ? <p className="text-danger">{labelErrorText(batch.error, t, errorText)}</p> : null}
    </Dialog>
  );
}
