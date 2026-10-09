import { useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { useTestLabel } from "@/lib/ipc/devices";
import { useFoundDevices } from "@/lib/ipc/hardware";
import type {
  Discovered,
  LabelDpi,
  LabelMedia,
  LabelPreset,
  LabelPrinterConfig,
  LabelProfile,
  Transport,
} from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useErrorText, useFieldError } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { newPrinterId, transportErrors } from "../PrinterForm";
import { BaudSelect, PortSelect } from "./SerialFields";

type Kind = "windows" | "tcp" | "serial";
type WindowsPrinter = Extract<Discovered, { kind: "windows_printer" }>;

const PRESETS: readonly LabelPreset[] = ["xprinter", "tsc", "smart_pos", "zebra", "godex"];
const MEDIAS: readonly LabelMedia[] = ["gap", "black_mark", "continuous"];
const DARKNESS = Array.from({ length: 16 }, (_, i) => i);
const SPEEDS = [1, 2, 3, 4, 5, 6];
/** 0 to 5 mm by half millimetres, in tenths of a millimetre (the profile's unit). */
const OFFSETS = Array.from({ length: 11 }, (_, i) => i * 5);

/** Mirrors pos-hw `LabelProfile::preset` (Task 6): TSPL at 203 dpi, darkness 8, speed 4, gap,
 * no offsets, a 0 bit black for Xprinter, TSC and Smart POS; ZPL without inversion for Zebra and
 * Godex. `hardware_save` validates the result again. */
export function labelProfile(preset: LabelPreset): LabelProfile {
  const tspl: LabelProfile = {
    preset,
    language: "tspl",
    dpi: "d203",
    darkness: 8,
    speed: 4,
    media: "gap",
    offsetXDmm: 0,
    offsetYDmm: 0,
    bitmapBlackIsZero: true,
  };
  return preset === "zebra" || preset === "godex"
    ? { ...tspl, language: "zpl", bitmapBlackIsZero: false }
    : tspl;
}

export function newLabelPrinter(): LabelPrinterConfig {
  return {
    id: newPrinterId(),
    name: "",
    transport: { kind: "windows", printerName: "" },
    profile: labelProfile("xprinter"),
  };
}

/** Client-side checks with the backend's field names (an empty name takes the default one). */
export function labelPrinterErrors(p: LabelPrinterConfig): Record<string, string> {
  return transportErrors(p.transport);
}

/** Windows printers from discovery, the likely label printers first. */
function labelPrintersFirst(found: readonly Discovered[] | undefined): WindowsPrinter[] {
  const windows = (found ?? []).flatMap((d) => (d.kind === "windows_printer" ? [d] : []));
  return [...windows.filter((d) => d.likelyLabel), ...windows.filter((d) => !d.likelyLabel)];
}

export function LabelPrinterForm({
  printer,
  onChange,
  errors,
}: {
  printer: LabelPrinterConfig;
  onChange: (printer: LabelPrinterConfig) => void;
  errors: Record<string, string>;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const err = useFieldError(errors);
  const found = useFoundDevices();
  const test = useTestLabel();
  const [listing, setListing] = useState(false);
  const transport = printer.transport;
  const [port, setPort] = useState(transport.kind === "tcp" ? String(transport.port) : "9100");
  const kind: Kind = transport.kind === "file" ? "windows" : transport.kind;
  const windows = labelPrintersFirst(found.data);
  const profile = printer.profile;
  const setTransport = (next: Transport) => onChange({ ...printer, transport: next });
  const setProfile = (change: Partial<LabelProfile>) =>
    onChange({ ...printer, profile: { ...profile, ...change } });
  const inverted = profile.bitmapBlackIsZero !== labelProfile(profile.preset).bitmapBlackIsZero;
  const offsets = (value: number) =>
    OFFSETS.includes(value) ? OFFSETS : [...OFFSETS, value].sort((a, b) => a - b);
  const mm = (dmm: number) => formatQty(dmm * 100, lang);

  return (
    <div className="flex flex-col gap-4">
      <Field id="label-printer-name" label={t("hardware:printer.name")} error={err("name")}>
        <Input
          id="label-printer-name"
          dir="auto"
          value={printer.name}
          maxLength={60}
          onChange={(e) => onChange({ ...printer, name: e.target.value })}
        />
      </Field>
      <RadioCards
        legend={t("hardware:printer.connection")}
        name="label-printer-kind"
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
      {transport.kind === "windows" ? (
        <>
          <div>
            <Button
              variant="outline"
              onClick={() => {
                setListing(true);
                void found.refetch();
              }}
            >
              {t("hardware:labelPrinters.detect")}
            </Button>
          </div>
          {found.error ? (
            <p role="alert" className="text-danger">
              {errorText(found.error)}
            </p>
          ) : null}
          {listing && windows.length > 0 ? (
            <div className="grid gap-2 sm:grid-cols-2">
              {windows.map((d) => (
                <button
                  key={d.name}
                  type="button"
                  aria-pressed={transport.printerName === d.name}
                  onClick={() => setTransport({ kind: "windows", printerName: d.name })}
                  className={cn(
                    "min-h-11 rounded-card border-2 p-3 text-start font-semibold",
                    transport.printerName === d.name
                      ? "border-brand bg-brand-soft"
                      : "border-line bg-surface",
                  )}
                >
                  {d.likelyLabel ? t("hardware:labelPrinters.likely", { name: d.name }) : d.name}
                </button>
              ))}
            </div>
          ) : null}
          <Field
            id="label-printer-windows"
            label={t("hardware:printer.windowsPrinter")}
            error={err("printerName")}
          >
            <Input
              id="label-printer-windows"
              dir="auto"
              value={transport.printerName}
              onChange={(e) => setTransport({ kind: "windows", printerName: e.target.value })}
            />
          </Field>
        </>
      ) : null}
      {transport.kind === "tcp" ? (
        <div className="grid grid-cols-2 gap-3">
          <Field id="label-printer-host" label={t("hardware:printer.host")} error={err("host")}>
            <Input
              id="label-printer-host"
              dir="ltr"
              value={transport.host}
              onChange={(e) => setTransport({ kind: "tcp", host: e.target.value, port: Number(port) || 0 })}
            />
          </Field>
          <Field id="label-printer-port" label={t("hardware:printer.port")} error={err("port")}>
            <NumericInput
              id="label-printer-port"
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
          <PortSelect
            id="label-printer-com"
            label={t("hardware:printer.serialPort")}
            none={t("hardware:printer.choose")}
            value={transport.port}
            onChange={(p) => setTransport({ ...transport, port: p })}
            error={err("serialPort")}
          />
          <BaudSelect
            id="label-printer-baud"
            label={t("hardware:printer.baud")}
            value={transport.baud}
            onChange={(baud) => setTransport({ ...transport, baud })}
          />
        </div>
      ) : null}
      <div className="grid gap-3 sm:grid-cols-2">
        <Field id="label-printer-preset" label={t("hardware:labelPrinters.preset")}>
          <NativeSelect
            id="label-printer-preset"
            value={profile.preset}
            onChange={(e) => onChange({ ...printer, profile: labelProfile(e.target.value as LabelPreset) })}
          >
            {PRESETS.map((p) => (
              <option key={p} value={p}>
                {t(`hardware:labelPrinters.presets.${p}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id="label-printer-dpi" label={t("hardware:labelPrinters.dpi")}>
          <NativeSelect
            id="label-printer-dpi"
            value={profile.dpi}
            onChange={(e) => setProfile({ dpi: e.target.value as LabelDpi })}
          >
            {(["d203", "d300"] as const).map((d) => (
              <option key={d} value={d}>
                {t(`hardware:labelPrinters.dpis.${d}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id="label-printer-darkness" label={t("hardware:labelPrinters.darkness")}>
          <NativeSelect
            id="label-printer-darkness"
            dir="ltr"
            value={profile.darkness}
            onChange={(e) => setProfile({ darkness: Number(e.target.value) })}
          >
            {DARKNESS.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id="label-printer-speed" label={t("hardware:labelPrinters.speed")}>
          <NativeSelect
            id="label-printer-speed"
            dir="ltr"
            value={profile.speed}
            onChange={(e) => setProfile({ speed: Number(e.target.value) })}
          >
            {SPEEDS.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id="label-printer-media" label={t("hardware:labelPrinters.media")}>
          <NativeSelect
            id="label-printer-media"
            value={profile.media}
            onChange={(e) => setProfile({ media: e.target.value as LabelMedia })}
          >
            {MEDIAS.map((m) => (
              <option key={m} value={m}>
                {t(`hardware:labelPrinters.medias.${m}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id="label-printer-offset-x" label={t("hardware:labelPrinters.offsetX")}>
          <NativeSelect
            id="label-printer-offset-x"
            dir="ltr"
            value={profile.offsetXDmm}
            onChange={(e) => setProfile({ offsetXDmm: Number(e.target.value) })}
          >
            {offsets(profile.offsetXDmm).map((o) => (
              <option key={o} value={o}>
                {mm(o)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id="label-printer-offset-y" label={t("hardware:labelPrinters.offsetY")}>
          <NativeSelect
            id="label-printer-offset-y"
            dir="ltr"
            value={profile.offsetYDmm}
            onChange={(e) => setProfile({ offsetYDmm: Number(e.target.value) })}
          >
            {offsets(profile.offsetYDmm).map((o) => (
              <option key={o} value={o}>
                {mm(o)}
              </option>
            ))}
          </NativeSelect>
        </Field>
      </div>
      <details className="rounded-card border border-line p-3">
        <summary className="cursor-pointer font-semibold">{t("hardware:printer.advanced")}</summary>
        <label className="mt-3 flex min-h-11 items-center gap-3 font-semibold">
          <Checkbox
            checked={inverted}
            onCheckedChange={() => setProfile({ bitmapBlackIsZero: !profile.bitmapBlackIsZero })}
          />
          {t("hardware:labelPrinters.polarity")}
        </label>
      </details>
      <div className="flex flex-col gap-2">
        <div>
          <Button
            variant="outline"
            disabled={test.isPending}
            onClick={() =>
              test.mutate({
                printer: { ...printer, name: printer.name.trim() || t("hardware:labelPrinters.defaultName") },
                lang,
              })
            }
          >
            {t("hardware:labelPrinters.test")}
          </Button>
        </div>
        {test.data ? (
          <p role="status" className="text-ok">
            {t("hardware:labelPrinters.tested", { printer: test.data.printer })}
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
