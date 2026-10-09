import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { LanguageToggle } from "@/components/language-toggle";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { type Lang, LANGS, isLang } from "@/lib/i18n";
import { useDocument } from "@/lib/ipc/documents";
import { useDiscoverDevices, useHardwareConfig } from "@/lib/ipc/hardware";
import { usePrintDocument } from "@/lib/ipc/print";
import type { DocType, Document, HardwareConfig } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";

const COPIES = [1, 2, 3, 4, 5] as const;

/**
 * Prints a document on A4 (DOC-018): its language or another one, a second language or one
 * language only, this PC's printer of the kind (or any Windows printer), 1 to 5 copies. The
 * backend decides original or DUPLICATA and says which in the toast. `doc` is the document when
 * the caller already has it (the one a validation just answered); else it is read.
 */
export function PrintDialog({
  id,
  docType,
  open,
  onClose,
  doc: given,
}: {
  id: string;
  docType: DocType;
  open: boolean;
  onClose: () => void;
  doc?: Document;
}) {
  const read = useDocument(open && !given ? id : null);
  const doc = given ?? read.data;
  const config = useHardwareConfig(open);
  const { mutate: discover, data: discovered } = useDiscoverDevices();
  useEffect(() => {
    if (open) discover();
  }, [open, discover]);
  // The form starts from the document and this PC's settings: it waits for both.
  if (!open || !doc || config.isPending) return null;
  const printers = (discovered ?? []).flatMap((d) => (d.kind === "windows_printer" ? [d.name] : []));
  return (
    <PrintForm
      doc={doc}
      docType={docType}
      config={config.data ?? null}
      printers={printers}
      onClose={onClose}
    />
  );
}

function PrintForm({
  doc,
  docType,
  config,
  printers,
  onClose,
}: {
  doc: Document;
  docType: DocType;
  config: HardwareConfig | null;
  printers: string[];
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const print = usePrintDocument();
  const rule = config?.documents?.kinds?.find((k) => k.docType === docType);
  const configured = rule?.printer ?? config?.documents?.defaultPrinter ?? null;
  const [lang, setLang] = useState<Lang>(doc.lang);
  const [second, setSecond] = useState<Lang | "">(
    doc.secondLang && doc.secondLang !== doc.lang ? doc.secondLang : "",
  );
  const [printer, setPrinter] = useState(configured ?? "");
  const [copies, setCopies] = useState(Math.min(Math.max(rule?.copies ?? 1, 1), 5));
  // This PC's printer of the kind first, then every Windows printer found; without a configured
  // one, Windows' default printer.
  const names = [...new Set([...(configured ? [configured] : []), ...printers])];
  const submit = () =>
    print.mutate(
      {
        id: doc.id,
        options: { lang, secondLang: second === "" ? null : second, monolingual: second === "" },
        printer: printer === "" ? null : printer,
        copies,
      },
      {
        onSuccess: (done) => {
          toast.success(
            t(
              done.dialog
                ? "billing:print.dialog"
                : done.duplicate
                  ? "billing:print.duplicate"
                  : "billing:print.original",
            ),
          );
          onClose();
        },
      },
    );
  return (
    <Dialog
      open
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("billing:print.title", { number: doc.number ?? "" })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={print.isPending} onClick={submit}>
            {t("billing:print.submit")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-1.5">
        <span className="font-semibold">{t("billing:print.language")}</span>
        <LanguageToggle
          value={lang}
          onChange={(l) => {
            setLang(l);
            if (second === l) setSecond("");
          }}
        />
      </div>
      <Field id="print-second" label={t("billing:print.second")}>
        <NativeSelect
          id="print-second"
          value={second}
          onChange={(event) => {
            const value = event.target.value;
            setSecond(isLang(value) ? value : "");
          }}
        >
          <option value="">{t("billing:print.monolingual")}</option>
          {LANGS.filter((l) => l !== lang).map((l) => (
            <option key={l} value={l} lang={l}>
              {t(`common:languages.${l}`)}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <Field id="print-printer" label={t("billing:print.printer")}>
        <NativeSelect id="print-printer" value={printer} onChange={(event) => setPrinter(event.target.value)}>
          {configured ? null : <option value="">{t("billing:print.defaultPrinter")}</option>}
          {names.map((name) => (
            <option key={name} value={name}>
              {name}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <Field id="print-copies" label={t("billing:print.copies")}>
        <NativeSelect
          id="print-copies"
          className="w-auto"
          value={String(copies)}
          onChange={(event) => setCopies(Number(event.target.value))}
        >
          {COPIES.map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </NativeSelect>
      </Field>
      {print.error ? (
        <p role="alert" className="text-danger">
          {errorText(print.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
