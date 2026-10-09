import { useState } from "react";
import { useTranslation } from "react-i18next";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { useSerialScans } from "@/lib/ipc/devices";
import type {
  HidSuffix,
  ScannerLayout,
  ScannerOptions,
  SerialScannerConfig,
  SerialSuffix,
} from "@/lib/ipc/types";
import { PREFIX_MAX } from "@/lib/scanner-learn";
import { ScannerTest } from "../ScannerTest";
import { DeviceDot } from "./DeviceDot";
import type { CardProps } from "./HardwareHub";
import { LearnScannerDialog } from "./LearnScannerDialog";
import { BaudSelect, eightN1, PortSelect } from "./SerialFields";

const SERIAL_SUFFIXES: readonly SerialSuffix[] = ["cr", "lf", "cr_lf", "tab", "silence"];

/** What `hardware_save` accepts in a prefix: printable ASCII (an AIM identifier such as `]E0`). */
function asciiPrefix(text: string): string {
  return text.replace(/[^\x20-\x7E]/g, "").slice(0, PREFIX_MAX);
}

/** "Lecteurs": the keyboard-wedge scanner (layout, prefix, end of scan, a learn mode) and a scanner
 * in USB-COM mode. */
export function ScannersCard({ config, onChange, errorFor }: CardProps) {
  const { t } = useTranslation();
  const [learning, setLearning] = useState(false);
  const scanner = config.scanner ?? {};
  const layout = scanner.layout ?? "us";
  const prefix = scanner.prefix ?? "";
  const suffix = scanner.suffix ?? "enter_or_tab";
  const set = (change: Partial<ScannerOptions>) =>
    onChange({ ...config, scanner: { ...scanner, ...change } });
  return (
    <>
      <Card>
        <h2 className="text-xl font-bold">{t("hardware:scanners.title")}</h2>
        <RadioCards
          legend={t("hardware:scanner.layout")}
          name="scanner-layout"
          value={layout}
          onChange={(next: ScannerLayout) => set({ layout: next })}
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
        <Field id="scanner-prefix" label={t("hardware:scanners.prefix")} error={errorFor?.("scanner.prefix")}>
          <Input
            id="scanner-prefix"
            dir="ltr"
            autoComplete="off"
            maxLength={PREFIX_MAX}
            value={prefix}
            onChange={(e) => set({ prefix: asciiPrefix(e.target.value) })}
          />
        </Field>
        <RadioCards
          legend={t("hardware:scanners.suffix")}
          name="scanner-suffix"
          value={suffix}
          onChange={(next: HidSuffix) => set({ suffix: next })}
          options={(["enter_or_tab", "silence"] as const).map((s) => ({
            value: s,
            label: t(`hardware:scanners.suffixes.${s}`),
          }))}
        />
        <div>
          <Button variant="outline" onClick={() => setLearning(true)}>
            {t("hardware:scanners.learn.open")}
          </Button>
        </div>
        <ScannerTest layout={layout} prefix={prefix} suffix={suffix} />
      </Card>
      <SerialScanner
        serial={scanner.serial ?? null}
        onChange={(serial) => set({ serial })}
        errorFor={errorFor}
      />
      {learning ? (
        <LearnScannerDialog
          onClose={() => setLearning(false)}
          onApply={(learned) => {
            set(learned);
            setLearning(false);
          }}
        />
      ) : null}
    </>
  );
}

function SerialScanner({
  serial,
  onChange,
  errorFor,
}: {
  serial: SerialScannerConfig | null;
  onChange: (serial: SerialScannerConfig | null) => void;
  errorFor: CardProps["errorFor"];
}) {
  const { t } = useTranslation();
  const [last, setLast] = useState<string | null>(null);
  useSerialScans(setLast, serial !== null);
  const set = (change: Partial<SerialScannerConfig>) => {
    if (serial) onChange({ ...serial, ...change });
  };
  return (
    <Card>
      <div className="flex items-center gap-3">
        <h2 className="text-xl font-bold">{t("hardware:scanners.serial.title")}</h2>
        <DeviceDot device="serial_scanner" />
      </div>
      <label className="flex min-h-11 items-center gap-3 font-semibold">
        <Checkbox
          checked={serial !== null}
          onCheckedChange={(v) =>
            onChange(v === true ? { serial: eightN1(""), prefix: "", suffix: "cr" } : null)
          }
        />
        {t("hardware:scanners.serial.enabled")}
      </label>
      {serial ? (
        <>
          <div className="grid gap-3 sm:grid-cols-2">
            <PortSelect
              id="serial-scanner-port"
              label={t("hardware:scanners.serial.port")}
              none={t("hardware:printer.choose")}
              value={serial.serial.port}
              onChange={(port) => set({ serial: { ...serial.serial, port } })}
              error={errorFor?.("scanner.serial.serial.port")}
            />
            <BaudSelect
              id="serial-scanner-baud"
              label={t("hardware:printer.baud")}
              value={serial.serial.baud}
              onChange={(baud) => set({ serial: { ...serial.serial, baud } })}
            />
            <Field
              id="serial-scanner-prefix"
              label={t("hardware:scanners.prefix")}
              error={errorFor?.("scanner.serial.prefix")}
            >
              <Input
                id="serial-scanner-prefix"
                dir="ltr"
                autoComplete="off"
                maxLength={PREFIX_MAX}
                value={serial.prefix}
                onChange={(e) => set({ prefix: asciiPrefix(e.target.value) })}
              />
            </Field>
            <Field id="serial-scanner-suffix" label={t("hardware:scanners.suffix")}>
              <NativeSelect
                id="serial-scanner-suffix"
                value={serial.suffix}
                onChange={(e) => set({ suffix: e.target.value as SerialSuffix })}
              >
                {SERIAL_SUFFIXES.map((s) => (
                  <option key={s} value={s}>
                    {t(`hardware:scanners.serial.suffixes.${s}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          </div>
          <p aria-live="polite" className="text-ink-2">
            {last !== null ? t("hardware:scanners.serial.last", { code: last }) : null}
          </p>
        </>
      ) : null}
    </Card>
  );
}
