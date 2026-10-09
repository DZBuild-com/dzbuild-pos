import { Printer } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { newPrinterId, presetProfile, withPaper } from "@/features/settings/PrinterForm";
import { useDiscoverDevices, useTestPrint } from "@/lib/ipc/hardware";
import type { PrinterConfig } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** Wizard step 4 (spec §10.5, ONB-009): detect, pick, test; saved when the wizard ends. */
export function PrinterStep({
  printer,
  onChange,
}: {
  printer: PrinterConfig | null;
  onChange: (printer: PrinterConfig | null) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const discover = useDiscoverDevices();
  const test = useTestPrint();
  const printers = (discover.data ?? []).flatMap((d) => (d.kind === "windows_printer" ? [d] : []));
  const pick = (name: string) =>
    onChange({
      id: printer?.id ?? newPrinterId(),
      name: t("hardware:printer.defaultName"),
      transport: { kind: "windows", printerName: name },
      profile: printer?.profile ?? presetProfile("generic", "mm80"),
    });
  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-3">
        <Printer aria-hidden className="size-10 shrink-0 text-ink-2" />
        <div>
          <h2 className="text-2xl font-bold">{t("setup:printer.title")}</h2>
          <p className="text-ink-2">{t("setup:printer.hint")}</p>
        </div>
      </div>
      <div>
        <Button variant="secondary" size="lg" disabled={discover.isPending} onClick={() => discover.mutate()}>
          {t("hardware:printer.discover")}
        </Button>
      </div>
      {discover.isSuccess && printers.length === 0 ? (
        <p className="text-ink-2">{t("setup:printer.none")}</p>
      ) : null}
      {discover.error ? (
        <p role="alert" className="text-danger">
          {errorText(discover.error)}
        </p>
      ) : null}
      {printers.length > 0 ? (
        <div
          role="radiogroup"
          aria-label={t("hardware:printer.windowsPrinter")}
          className="grid gap-2 sm:grid-cols-2"
        >
          {printers.map((d) => {
            const selected =
              printer?.transport.kind === "windows" && printer.transport.printerName === d.name;
            return (
              <button
                key={d.name}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => pick(d.name)}
                className={cn(
                  "flex min-h-16 flex-col items-start justify-center rounded-card border-2 p-3 text-start",
                  selected ? "border-brand bg-brand-soft" : "border-line bg-surface",
                )}
              >
                <span className="font-semibold">{d.name}</span>
                {d.likelyReceipt ? (
                  <span className="text-sm text-ok">{t("setup:printer.likely")}</span>
                ) : null}
              </button>
            );
          })}
        </div>
      ) : null}
      {printer ? (
        <div className="flex flex-wrap items-center gap-2">
          {(["mm58", "mm80"] as const).map((w) => (
            <Button
              key={w}
              variant="outline"
              aria-pressed={printer.profile.width === w}
              className={cn(printer.profile.width === w && "border-brand bg-brand-soft")}
              onClick={() => onChange({ ...printer, profile: withPaper(printer.profile, w) })}
            >
              {w === "mm58" ? "58 mm" : "80 mm"}
            </Button>
          ))}
          <Button variant="outline" disabled={test.isPending} onClick={() => test.mutate({ printer, lang })}>
            {t("hardware:printer.test")}
          </Button>
        </div>
      ) : null}
      {test.data ? (
        <p role="status" className="text-ok">
          {t("hardware:printer.testSent", { printer: test.data.printer })}
        </p>
      ) : null}
      {test.error ? (
        <p role="alert" className="text-danger">
          {errorText(test.error)}
        </p>
      ) : null}
    </div>
  );
}
