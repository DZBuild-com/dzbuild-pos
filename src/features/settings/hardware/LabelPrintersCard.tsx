import { Plus } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import type { LabelPrinterConfig } from "@/lib/ipc/types";
import type { CardProps } from "./HardwareHub";
import { LabelPrinterForm, labelPrinterErrors, newLabelPrinter } from "./LabelPrinterForm";

/** "Étiquettes": this PC's label printers (HW-013) and the default one. The Arabic-label rule
 * (DOC-034, `labels.arabic_categories`) is a shop setting: it lives with the document settings. */
export function LabelPrintersCard({ config, onChange }: CardProps) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState<LabelPrinterConfig | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const printers = config.labelPrinters ?? [];
  const defaultId = config.labelPrinterId ?? null;
  const edit = (printer: LabelPrinterConfig) => {
    setErrors({});
    setEditing(printer);
  };
  const keep = () => {
    if (!editing) return;
    const named =
      editing.name.trim() === "" ? { ...editing, name: t("hardware:labelPrinters.defaultName") } : editing;
    const found = labelPrinterErrors(named);
    setErrors(found);
    if (Object.keys(found).length > 0) {
      setEditing(named);
      return;
    }
    const exists = printers.some((p) => p.id === named.id);
    const next = exists ? printers.map((p) => (p.id === named.id ? named : p)) : [...printers, named];
    onChange({ ...config, labelPrinters: next, labelPrinterId: defaultId ?? named.id });
    setEditing(null);
  };
  const remove = (id: string) =>
    onChange({
      ...config,
      labelPrinters: printers.filter((p) => p.id !== id),
      labelPrinterId: defaultId === id ? null : defaultId,
    });

  return (
    <Card>
      <h2 className="text-xl font-bold">{t("hardware:labelPrinters.title")}</h2>
      {printers.length === 0 ? (
        <p className="text-ink-2">{t("hardware:labelPrinters.empty")}</p>
      ) : (
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 font-semibold">{t("hardware:labelPrinters.default")}</legend>
          <ul className="flex flex-col gap-2">
            {printers.map((p) => (
              <li
                key={p.id}
                className="flex flex-wrap items-center justify-between gap-2 rounded-card border border-line p-3"
              >
                <label className="flex min-h-11 items-center gap-2 font-semibold">
                  <input
                    type="radio"
                    name="label-printer"
                    className="size-5 accent-brand"
                    checked={defaultId === p.id}
                    onChange={() => onChange({ ...config, labelPrinterId: p.id })}
                  />
                  {p.name}
                </label>
                <div className="flex gap-2">
                  <Button variant="outline" onClick={() => edit(p)}>
                    {t("common:actions.edit")}
                  </Button>
                  <Button variant="ghost" onClick={() => remove(p.id)}>
                    {t("common:actions.remove")}
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        </fieldset>
      )}
      <div>
        <Button variant="secondary" onClick={() => edit(newLabelPrinter())}>
          <Plus aria-hidden />
          {t("hardware:labelPrinters.add")}
        </Button>
      </div>
      <p className="text-sm text-ink-2">
        {t("hardware:labelPrinters.arabicRule")}{" "}
        <Link to="/settings/documents" className="font-semibold text-info underline">
          {t("hardware:labelPrinters.arabicRuleLink")}
        </Link>
      </p>
      {editing ? (
        <Dialog
          open
          size="lg"
          onOpenChange={(open) => {
            if (!open) setEditing(null);
          }}
          title={t("hardware:labelPrinters.editTitle")}
          footer={
            <>
              <Button variant="outline" onClick={() => setEditing(null)}>
                {t("common:actions.cancel")}
              </Button>
              <Button variant="primary" onClick={keep}>
                {t("hardware:labelPrinters.keep")}
              </Button>
            </>
          }
        >
          <LabelPrinterForm printer={editing} onChange={setEditing} errors={errors} />
        </Dialog>
      ) : null}
    </Card>
  );
}
