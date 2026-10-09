import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { useSession } from "@/lib/ipc/auth";
import { useHardwareConfig } from "@/lib/ipc/hardware";
import { useBuildLabels, useLabelPreview, useLabelTemplates, usePrintLabels } from "@/lib/ipc/labels";
import type { LabelLang, LabelRequest, LabelSource } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { SheetDialog } from "./SheetDialog";
import { useTemplateName } from "./template-name";
import { labelErrorText, productsWith } from "./warnings";

export const LABEL_LANGS: readonly LabelLang[] = ["ar_fr", "ar", "fr", "en", "ar_en", "fr_en"];

/** pos-core `labels::MAX_COPIES`. */
const MAX_COPIES = 999;

/** The copies every item of a selection shares, "" when they differ. */
function sharedCopies(source: LabelSource): string {
  if (source.kind !== "products") return "";
  const counts = new Set(source.items.map((item) => item.copies));
  const [only] = counts;
  return counts.size === 1 && only !== undefined ? String(only) : "";
}

/**
 * LBL-006: the labels of a selection, a goods receipt, a count or the reprint queue, on this PC's
 * label printer. Rust builds the batch (prices, names, barcodes, the DOC-034 guard): the dialog
 * shows its warnings and the first label's preview, and never computes a price. Food and
 * regulated goods print without Arabic only after an explicit confirmation, journaled by Rust.
 * "Planche A4…" puts the same labels on an A4 sheet instead (`SheetDialog`).
 */
export function PrintLabelsDialog({ source, onClose }: { source: LabelSource; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const templateName = useTemplateName();
  const session = useSession(true);
  const templates = useLabelTemplates();
  const hardware = useHardwareConfig();
  const [templateId, setTemplateId] = useState<string | null>(null);
  const [lang, setLang] = useState<LabelLang | null>(null);
  const [copies, setCopies] = useState(() => sharedCopies(source));
  const [printerId, setPrinterId] = useState<string | null>(null);
  const [assign, setAssign] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [onSheet, setOnSheet] = useState(false);
  // "Copies de chaque article" applies to every product of a selection; the other sources give
  // one label per piece received or counted.
  const each = Number(copies);
  const copiesValid = Number.isInteger(each) && each >= 1 && each <= MAX_COPIES;
  const request: LabelRequest = {
    source:
      source.kind === "products" && copiesValid
        ? { kind: "products", items: source.items.map((item) => ({ ...item, copies: each })) }
        : source,
    templateId,
    lang,
  };
  const batch = useBuildLabels(request);
  const first = batch.data?.groups[0];
  const firstLabel = first?.labels[0];
  const preview = useLabelPreview(
    first
      ? { template: first.template, productId: firstLabel?.productId ?? null, lang: firstLabel?.lang ?? null }
      : null,
  );
  const print = usePrintLabels();
  const permissions = session.data?.permissions ?? [];
  const canEdit = permissions.includes("products.edit");
  const warnings = batch.data?.warnings ?? [];
  const arabicRequired = productsWith(warnings, "arabic_required");
  const arabicNameMissing = productsWith(warnings, "arabic_name_missing");
  const noBarcode = productsWith(warnings, "no_barcode");
  const noPrice = productsWith(warnings, "no_price");
  const guarded = arabicRequired + arabicNameMissing > 0;
  // what the preview laid out, then what the printer took
  const dropped = [...(preview.data?.warnings ?? []), ...(print.data?.warnings ?? [])].some(
    (w) => w.code === "lot_or_expiry_dropped",
  );
  const printers = hardware.data?.labelPrinters ?? [];
  const defaultPrinter = printers.find((p) => p.id === hardware.data?.labelPrinterId) ?? printers[0];
  const send = (arabicOverride: boolean) =>
    print.mutate(
      { request, printerId, assignMissingBarcodes: assign && canEdit, arabicOverride },
      { onSuccess: () => setConfirming(false) },
    );
  const choose = (change: () => void) => {
    change();
    setConfirming(false);
    print.reset();
  };

  // LBL-013: the same labels on an A4 planche; closing it comes back here.
  if (onSheet) return <SheetDialog request={request} onClose={() => setOnSheet(false)} />;

  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("labels:print.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="secondary" disabled={!batch.data} onClick={() => setOnSheet(true)}>
            {t("labels:print.sheet")}
          </Button>
          <Button
            variant="primary"
            disabled={!batch.data || printers.length === 0 || guarded || print.isPending}
            onClick={() => send(false)}
          >
            {t("labels:print.go", { count: batch.data?.total ?? 0 })}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 md:grid-cols-2">
        <div className="flex flex-col gap-3">
          <Field id="label-template" label={t("labels:print.template")}>
            <NativeSelect
              id="label-template"
              value={templateId ?? ""}
              onChange={(e) => choose(() => setTemplateId(e.target.value || null))}
            >
              <option value="">{t("labels:print.byCategory")}</option>
              {(templates.data?.templates ?? []).map((tpl) => (
                <option key={tpl.id} value={tpl.id}>
                  {templateName(tpl)}
                </option>
              ))}
            </NativeSelect>
          </Field>
          <Field id="label-lang" label={t("labels:print.lang")}>
            <NativeSelect
              id="label-lang"
              value={lang ?? ""}
              onChange={(e) => choose(() => setLang(LABEL_LANGS.find((l) => l === e.target.value) ?? null))}
            >
              <option value="">{t("labels:print.templateLang")}</option>
              {LABEL_LANGS.map((l) => (
                <option key={l} value={l}>
                  {t(`labels:langs.${l}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
          {source.kind === "products" ? (
            <Field id="label-copies" label={t("labels:print.copies")}>
              <NumericInput
                id="label-copies"
                kind="integer"
                aria-invalid={!copiesValid}
                value={copies}
                onValueChange={(value) => choose(() => setCopies(value))}
              />
            </Field>
          ) : null}
          {printers.length === 0 ? (
            <div className="flex flex-col gap-2">
              <p className="text-warn">{t("labels:print.noPrinter")}</p>
              {permissions.includes("settings.hardware") ? (
                <Button asChild variant="outline" className="self-start">
                  <Link to="/settings/hardware" onClick={onClose}>
                    {t("labels:print.openHardware")}
                  </Link>
                </Button>
              ) : null}
            </div>
          ) : printers.length > 1 ? (
            <Field id="label-printer" label={t("labels:print.printer")}>
              <NativeSelect
                id="label-printer"
                value={printerId ?? defaultPrinter?.id ?? ""}
                onChange={(e) => setPrinterId(e.target.value || null)}
              >
                {printers.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          ) : (
            <p>
              <span className="font-semibold">{t("labels:print.printer")}</span> {defaultPrinter?.name}
            </p>
          )}
        </div>
        {preview.data ? (
          <img
            src={preview.data.png}
            alt={t("labels:designer.previewAlt")}
            className="max-h-48 justify-self-center border border-line bg-surface"
          />
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
              <Button variant="danger" disabled={print.isPending} onClick={() => send(true)}>
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
              <Button variant="outline" disabled={printers.length === 0} onClick={() => setConfirming(true)}>
                {t("labels:print.withoutArabic")}
              </Button>
            </div>
          )}
        </div>
      ) : null}
      {noBarcode > 0 && canEdit ? (
        <label className="flex items-center gap-3 font-semibold">
          <Checkbox checked={assign} onCheckedChange={(v) => setAssign(v === true)} />
          {t("labels:print.assignBarcodes", { count: noBarcode })}
        </label>
      ) : null}
      {noBarcode > 0 && !canEdit ? (
        <p className="text-ink-2">{t("labels:print.noBarcode", { count: noBarcode })}</p>
      ) : null}
      {noPrice > 0 ? <p className="text-warn">{t("labels:print.noPrice", { count: noPrice })}</p> : null}
      {dropped ? <p className="text-warn">{t("labels:print.dropped")}</p> : null}
      {print.isSuccess ? (
        <p role="status" className="font-semibold text-ok">
          {t("labels:print.done", { count: print.data.labels, printer: print.data.outcome.printer })}
        </p>
      ) : null}
      {print.error ? <p className="text-danger">{labelErrorText(print.error, t, errorText)}</p> : null}
      {batch.error ? <p className="text-danger">{labelErrorText(batch.error, t, errorText)}</p> : null}
    </Dialog>
  );
}
