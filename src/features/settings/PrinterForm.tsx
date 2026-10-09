import { useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { useDiscoverDevices, useTestPrint } from "@/lib/ipc/hardware";
import type {
  CodePage,
  CutMode,
  DrawerPin,
  PaperWidth,
  Preset,
  PrintMode,
  PrinterConfig,
  Profile,
  RenderMode,
  Transport,
} from "@/lib/ipc/types";
import { useErrorText, useFieldError } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

type Kind = "windows" | "tcp" | "serial";

/** pos-hw `SERIAL_BAUDS`: the speeds `hardware_save` accepts. */
const BAUDS = [9_600, 19_200, 38_400, 57_600, 115_200, 4_800];

/** The head's dots and the Font A columns that fit them (pos-hw checks columns × 12 ≤ dots). */
const PAPER: Record<PaperWidth, { dots: number; columns: number }> = {
  mm58: { dots: 384, columns: 32 },
  mm80: { dots: 576, columns: 48 },
};

/** Mirrors pos-hw `Profile::preset` (Task 1): the backend validates the result again. */
export function presetProfile(preset: Preset, width: PaperWidth): Profile {
  const base: Profile = {
    preset,
    width,
    ...PAPER[width],
    render: "auto",
    codePage: "pc858",
    codePageSelector: 19,
    cut: "partial",
    feedBeforeCut: 4,
    drawerPin: "pin2",
    bandRows: 128,
    exitChineseMode: false,
  };
  if (preset === "xprinter") return { ...base, exitChineseMode: true, bandRows: 96 };
  if (preset === "epson") return { ...base, feedBeforeCut: 3 };
  if (preset === "rongta") return { ...base, exitChineseMode: true, bandRows: 64 };
  return base;
}

/** Another paper width: dots and columns follow it, the advanced settings stay as chosen. */
export function withPaper(profile: Profile, width: PaperWidth): Profile {
  return { ...profile, width, ...PAPER[width] };
}

/** Unique on this PC is enough (`hardware_save` checks it); no WebCrypto needed. */
export function newPrinterId(): string {
  return `prn-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function newPrinter(): PrinterConfig {
  return {
    id: newPrinterId(),
    name: "",
    transport: { kind: "windows", printerName: "" },
    profile: presetProfile("generic", "mm80"),
    mode: "raw",
    paperWidthMm: 80,
  };
}

/** pos-hw `ROLL_WIDTHS_MM`: the rolls a printer that only takes pages prints the receipt on. */
const ROLL_WIDTHS = [58, 80] as const;

/** Client-side checks with the backend's field names; `hardware_save` checks everything again. */
export function printerErrors(p: PrinterConfig): Record<string, string> {
  const errors = transportErrors(p.transport);
  if (p.name.trim() === "") errors.name = "validation:required";
  return errors;
}

/** The connection's checks, shared by receipt and label printers. */
export function transportErrors(t: Transport): Record<string, string> {
  const errors: Record<string, string> = {};
  if (t.kind === "windows" && t.printerName.trim() === "") errors.printerName = "validation:required";
  if (t.kind === "tcp" && t.host.trim() === "") errors.host = "validation:required";
  if (t.kind === "tcp" && !(t.port >= 1 && t.port <= 65_535)) errors.port = "hardware:errors.port";
  if (t.kind === "serial" && t.port.trim() === "") errors.serialPort = "validation:required";
  return errors;
}

export function PrinterForm({
  printer,
  onChange,
  errors,
}: {
  printer: PrinterConfig;
  onChange: (printer: PrinterConfig) => void;
  errors: Record<string, string>;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const err = useFieldError(errors);
  const discover = useDiscoverDevices();
  const test = useTestPrint();
  const transport = printer.transport;
  const [port, setPort] = useState(transport.kind === "tcp" ? String(transport.port) : "9100");
  const found = discover.data ?? [];
  const windows = found.flatMap((d) => (d.kind === "windows_printer" ? [d] : []));
  const serial = found.flatMap((d) => (d.kind === "serial_port" ? [d] : []));
  const kind: Kind = transport.kind === "file" ? "windows" : transport.kind;
  // HW-002: only a Windows printer has a driver to print pages through (pos-hw refuses the rest)
  const setTransport = (next: Transport) =>
    onChange({ ...printer, transport: next, ...(next.kind === "windows" ? {} : { mode: "raw" as const }) });
  const setProfile = (change: Partial<Profile>) =>
    onChange({ ...printer, profile: { ...printer.profile, ...change } });

  return (
    <div className="flex flex-col gap-4">
      <Field id="printer-name" label={t("hardware:printer.name")} error={err("name")}>
        <Input
          id="printer-name"
          dir="auto"
          value={printer.name}
          maxLength={60}
          onChange={(e) => onChange({ ...printer, name: e.target.value })}
        />
      </Field>
      <RadioCards
        legend={t("hardware:printer.connection")}
        name="printer-kind"
        value={kind}
        onChange={(k) =>
          setTransport(
            k === "windows"
              ? { kind: "windows", printerName: "" }
              : k === "tcp"
                ? { kind: "tcp", host: "", port: Number(port) || 9100 }
                : { kind: "serial", port: "", baud: 9_600 },
          )
        }
        options={(["windows", "tcp", "serial"] as const).map((k) => ({
          value: k,
          label: t(`hardware:printer.kinds.${k}`),
        }))}
      />
      <div>
        <Button variant="outline" disabled={discover.isPending} onClick={() => discover.mutate()}>
          {t("hardware:printer.discover")}
        </Button>
      </div>
      {discover.error ? (
        <p role="alert" className="text-danger">
          {errorText(discover.error)}
        </p>
      ) : null}
      {transport.kind === "windows" ? (
        <Field id="printer-windows" label={t("hardware:printer.windowsPrinter")} error={err("printerName")}>
          {windows.length > 0 ? (
            <NativeSelect
              id="printer-windows"
              value={transport.printerName}
              onChange={(e) => setTransport({ kind: "windows", printerName: e.target.value })}
            >
              <option value="">{t("hardware:printer.choose")}</option>
              {windows.map((d) => (
                <option key={d.name} value={d.name}>
                  {d.likelyReceipt ? t("hardware:printer.likely", { name: d.name }) : d.name}
                </option>
              ))}
            </NativeSelect>
          ) : (
            <Input
              id="printer-windows"
              dir="auto"
              value={transport.printerName}
              onChange={(e) => setTransport({ kind: "windows", printerName: e.target.value })}
            />
          )}
        </Field>
      ) : null}
      {transport.kind === "windows" ? (
        <RadioCards
          legend={t("config:hardware.mode")}
          name="printer-mode"
          value={printer.mode ?? "raw"}
          onChange={(mode: PrintMode) => onChange({ ...printer, mode })}
          options={(["raw", "driver"] as const).map((m) => ({
            value: m,
            label: t(`config:hardware.modes.${m}`),
            hint: t(`config:hardware.modes.${m}Hint`),
          }))}
        />
      ) : null}
      {transport.kind === "windows" && printer.mode === "driver" ? (
        <RadioCards
          legend={t("config:hardware.rollWidth")}
          name="printer-roll"
          value={String(printer.paperWidthMm ?? 80)}
          onChange={(width: string) => onChange({ ...printer, paperWidthMm: Number(width) })}
          options={ROLL_WIDTHS.map((w) => ({ value: String(w), label: `${w} mm` }))}
        />
      ) : null}
      {transport.kind === "tcp" ? (
        <div className="grid grid-cols-2 gap-3">
          <Field id="printer-host" label={t("hardware:printer.host")} error={err("host")}>
            <Input
              id="printer-host"
              dir="ltr"
              value={transport.host}
              onChange={(e) => setTransport({ kind: "tcp", host: e.target.value, port: Number(port) || 0 })}
            />
          </Field>
          <Field id="printer-port" label={t("hardware:printer.port")} error={err("port")}>
            <NumericInput
              id="printer-port"
              kind="integer"
              value={port}
              onValueChange={(v) => {
                setPort(v);
                setTransport({ kind: "tcp", host: transport.host, port: Number(v || "0") });
              }}
            />
          </Field>
        </div>
      ) : null}
      {transport.kind === "serial" ? (
        <div className="grid grid-cols-2 gap-3">
          <Field id="printer-com" label={t("hardware:printer.serialPort")} error={err("serialPort")}>
            <Input
              id="printer-com"
              dir="ltr"
              list="printer-com-list"
              value={transport.port}
              onChange={(e) => setTransport({ ...transport, port: e.target.value })}
            />
            <datalist id="printer-com-list">
              {serial.map((d) => (
                <option key={d.port} value={d.port}>
                  {d.description ?? d.port}
                </option>
              ))}
            </datalist>
          </Field>
          <Field id="printer-baud" label={t("hardware:printer.baud")}>
            <NativeSelect
              id="printer-baud"
              dir="ltr"
              value={transport.baud}
              onChange={(e) => setTransport({ ...transport, baud: Number(e.target.value) })}
            >
              {BAUDS.map((b) => (
                <option key={b} value={b}>
                  {b}
                </option>
              ))}
            </NativeSelect>
          </Field>
        </div>
      ) : null}
      <RadioCards
        legend={t("hardware:printer.paper")}
        name="printer-width"
        value={printer.profile.width}
        onChange={(width: PaperWidth) => onChange({ ...printer, profile: withPaper(printer.profile, width) })}
        options={[
          { value: "mm58" as const, label: "58 mm" },
          { value: "mm80" as const, label: "80 mm" },
        ]}
      />
      <details className="rounded-card border border-line p-3">
        <summary className="cursor-pointer font-semibold">{t("hardware:printer.advanced")}</summary>
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
          <Field id="printer-preset" label={t("hardware:printer.preset")}>
            <NativeSelect
              id="printer-preset"
              value={printer.profile.preset}
              onChange={(e) =>
                onChange({
                  ...printer,
                  profile: presetProfile(e.target.value as Preset, printer.profile.width),
                })
              }
            >
              {(["generic", "xprinter", "epson", "rongta"] as const).map((p) => (
                <option key={p} value={p}>
                  {t(`hardware:printer.presets.${p}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
          <Field id="printer-render" label={t("hardware:printer.render")}>
            <NativeSelect
              id="printer-render"
              value={printer.profile.render}
              onChange={(e) => setProfile({ render: e.target.value as RenderMode })}
            >
              {(["auto", "text", "raster"] as const).map((r) => (
                <option key={r} value={r}>
                  {t(`hardware:printer.renders.${r}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
          <Field id="printer-codepage" label={t("hardware:printer.codePage")}>
            <NativeSelect
              id="printer-codepage"
              dir="ltr"
              value={printer.profile.codePage}
              onChange={(e) => {
                const codePage = e.target.value as CodePage;
                setProfile({ codePage, codePageSelector: codePage === "pc858" ? 19 : 16 });
              }}
            >
              <option value="pc858">PC858</option>
              <option value="wpc1252">Windows-1252</option>
            </NativeSelect>
          </Field>
          <Field id="printer-selector" label={t("hardware:printer.codePageSelector")}>
            <NumericInput
              id="printer-selector"
              kind="integer"
              value={String(printer.profile.codePageSelector)}
              onValueChange={(v) => setProfile({ codePageSelector: Math.min(255, Number(v || "0")) })}
            />
          </Field>
          <Field id="printer-cut" label={t("hardware:printer.cut")}>
            <NativeSelect
              id="printer-cut"
              value={printer.profile.cut}
              onChange={(e) => setProfile({ cut: e.target.value as CutMode })}
            >
              {(["partial", "full", "none"] as const).map((c) => (
                <option key={c} value={c}>
                  {t(`hardware:printer.cuts.${c}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
          <Field id="printer-pin" label={t("hardware:printer.drawerPin")}>
            <NativeSelect
              id="printer-pin"
              value={printer.profile.drawerPin}
              onChange={(e) => setProfile({ drawerPin: e.target.value as DrawerPin })}
            >
              <option value="pin2">{t("hardware:printer.pins.pin2")}</option>
              <option value="pin5">{t("hardware:printer.pins.pin5")}</option>
            </NativeSelect>
          </Field>
          <Field id="printer-band" label={t("hardware:printer.bandRows")}>
            <NumericInput
              id="printer-band"
              kind="integer"
              value={String(printer.profile.bandRows)}
              onValueChange={(v) => setProfile({ bandRows: Number(v || "0") })}
            />
          </Field>
          <label className="flex min-h-11 items-center gap-3 font-semibold">
            <Checkbox
              checked={printer.profile.exitChineseMode}
              onCheckedChange={(v) => setProfile({ exitChineseMode: v === true })}
            />
            {t("hardware:printer.chineseMode")}
          </label>
        </div>
      </details>
      <div className="flex flex-col gap-2">
        <div>
          <Button
            variant="outline"
            disabled={test.isPending}
            onClick={() =>
              test.mutate({
                printer: { ...printer, name: printer.name || t("hardware:printer.defaultName") },
                lang,
              })
            }
          >
            {t("hardware:printer.test")}
          </Button>
        </div>
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
    </div>
  );
}
