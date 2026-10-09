import { Plus } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { RadioCards } from "@/components/radio-cards";
import { Splash } from "@/components/splash";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { NativeSelect } from "@/components/ui/native-select";
import { isLang } from "@/lib/i18n";
import { useHardwareConfig, useSaveHardware } from "@/lib/ipc/hardware";
import type { HardwareConfig, PrinterConfig, ReceiptOptions, ScannerLayout } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { newPrinter, PrinterForm, printerErrors } from "./PrinterForm";
import { ScannerTest } from "./ScannerTest";

/** The defaults pos-hw fills in for a missing field (`ReceiptOptions::default`): the bindings mark
 * every field optional (serde `default`). */
const RECEIPT_DEFAULTS: Required<ReceiptOptions> = {
  printByDefault: true,
  openDrawerOnCash: true,
  drawerBeforePrint: false,
  logo: true,
  secondLang: null,
};

const RECEIPT_BOXES = ["printByDefault", "openDrawerOnCash", "drawerBeforePrint", "logo"] as const;

type Props =
  | { embedded: true; config: HardwareConfig; onChange: (config: HardwareConfig) => void }
  | { embedded?: false; config?: undefined; onChange?: undefined };

/** Réglages › Matériel › "Imprimantes de tickets": this PC's receipt printers and receipt options.
 * Embedded in the hardware hub it edits the hub's draft (the hub saves); standalone (M1b) it loads
 * `hardware.json`, adds the scanner layout and saves on its own. */
export function HardwareSettings(props: Props) {
  if (props.embedded) return <ReceiptPrinters config={props.config} onChange={props.onChange} />;
  return <Standalone />;
}

function Standalone() {
  const errorText = useErrorText();
  const loaded = useHardwareConfig();
  if (loaded.isPending) return <Splash inline />;
  if (loaded.isError) {
    return (
      <p role="alert" className="text-danger">
        {errorText(loaded.error)}
      </p>
    );
  }
  // Loaded once: the screen edits a draft that only `hardware_save` writes back.
  return <StandaloneForm initial={loaded.data} />;
}

function StandaloneForm({ initial }: { initial: HardwareConfig }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const save = useSaveHardware();
  const [config, setConfig] = useState<HardwareConfig>(initial);
  const scanner = config.scanner ?? {};
  return (
    <div className="flex flex-col gap-6">
      <ReceiptPrinters config={config} onChange={setConfig} />
      <Card>
        <h2 className="text-xl font-bold">{t("hardware:scanner.title")}</h2>
        <RadioCards
          legend={t("hardware:scanner.layout")}
          name="scanner-layout"
          value={scanner.layout ?? "us"}
          onChange={(layout: ScannerLayout) => setConfig({ ...config, scanner: { ...scanner, layout } })}
          options={[
            {
              value: "us" as const,
              label: t("hardware:scanner.layouts.us"),
              hint: t("hardware:scanner.layouts.usHint"),
            },
            {
              value: "azerty" as const,
              label: t("hardware:scanner.layouts.azerty"),
              hint: t("hardware:scanner.layouts.azertyHint"),
            },
          ]}
        />
        <ScannerTest layout={scanner.layout ?? "us"} prefix={scanner.prefix} suffix={scanner.suffix} />
      </Card>
      <div className="flex flex-col gap-2">
        <div>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(config)}>
            {t("hardware:save")}
          </Button>
        </div>
        {save.isSuccess ? (
          <p role="status" className="text-ok">
            {t("hardware:saved")}
          </p>
        ) : null}
        {save.error ? (
          <p role="alert" className="text-danger">
            {errorText(save.error)}
          </p>
        ) : null}
      </div>
    </div>
  );
}

function ReceiptPrinters({
  config,
  onChange,
}: {
  config: HardwareConfig;
  onChange: (config: HardwareConfig) => void;
}) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState<PrinterConfig | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const printers = config.printers ?? [];
  const receiptPrinterId = config.receiptPrinterId ?? null;
  const receipt = { ...RECEIPT_DEFAULTS, ...config.receipt };
  const setReceipt = (change: Partial<ReceiptOptions>) =>
    onChange({ ...config, receipt: { ...receipt, ...change } });
  const edit = (printer: PrinterConfig) => {
    setErrors({});
    setEditing(printer);
  };
  const keepPrinter = () => {
    if (!editing) return;
    // an unnamed printer takes the default name ("Caisse 1"), as the test page already did
    const named =
      editing.name.trim() === "" ? { ...editing, name: t("hardware:printer.defaultName") } : editing;
    const found = printerErrors(named);
    setErrors(found);
    if (Object.keys(found).length > 0) {
      setEditing(named);
      return;
    }
    const exists = printers.some((p) => p.id === named.id);
    const next = exists ? printers.map((p) => (p.id === named.id ? named : p)) : [...printers, named];
    onChange({ ...config, printers: next, receiptPrinterId: receiptPrinterId ?? named.id });
    setEditing(null);
  };
  const remove = (id: string) =>
    onChange({
      ...config,
      printers: printers.filter((p) => p.id !== id),
      receiptPrinterId: receiptPrinterId === id ? null : receiptPrinterId,
    });

  return (
    <>
      <Card>
        <h2 className="text-xl font-bold">{t("hardware:printers.title")}</h2>
        {printers.length === 0 ? <p className="text-ink-2">{t("hardware:printers.empty")}</p> : null}
        <ul className="flex flex-col gap-2">
          {printers.map((p) => (
            <li
              key={p.id}
              className="flex flex-wrap items-center justify-between gap-2 rounded-card border border-line p-3"
            >
              <label className="flex min-h-11 items-center gap-2 font-semibold">
                <input
                  type="radio"
                  name="receipt-printer"
                  className="size-5 accent-brand"
                  checked={receiptPrinterId === p.id}
                  onChange={() => onChange({ ...config, receiptPrinterId: p.id })}
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
        <div>
          <Button variant="secondary" onClick={() => edit(newPrinter())}>
            <Plus aria-hidden />
            {t("hardware:printers.add")}
          </Button>
        </div>
      </Card>
      <Card>
        <h2 className="text-xl font-bold">{t("hardware:receipt.title")}</h2>
        {RECEIPT_BOXES.map((key) => (
          <label key={key} className="flex min-h-11 items-center gap-3 font-semibold">
            <Checkbox checked={receipt[key]} onCheckedChange={(v) => setReceipt({ [key]: v === true })} />
            {t(`hardware:receipt.${key}`)}
          </label>
        ))}
        <div className="flex flex-col gap-1.5">
          <label htmlFor="second-lang" className="font-semibold">
            {t("hardware:receipt.secondLang")}
          </label>
          <NativeSelect
            id="second-lang"
            value={receipt.secondLang ?? ""}
            onChange={(e) => setReceipt({ secondLang: isLang(e.target.value) ? e.target.value : null })}
          >
            <option value="">{t("hardware:receipt.none")}</option>
            <option value="ar">{t("common:languages.ar")}</option>
            <option value="fr">{t("common:languages.fr")}</option>
            <option value="en">{t("common:languages.en")}</option>
          </NativeSelect>
        </div>
      </Card>
      {editing ? (
        <Dialog
          open
          size="lg"
          onOpenChange={(open) => {
            if (!open) setEditing(null);
          }}
          title={t("hardware:printer.title")}
          footer={
            <>
              <Button variant="outline" onClick={() => setEditing(null)}>
                {t("common:actions.cancel")}
              </Button>
              <Button variant="primary" onClick={keepPrinter}>
                {t("hardware:printer.keep")}
              </Button>
            </>
          }
        >
          <PrinterForm printer={editing} onChange={setEditing} errors={errors} />
        </Dialog>
      ) : null}
    </>
  );
}
