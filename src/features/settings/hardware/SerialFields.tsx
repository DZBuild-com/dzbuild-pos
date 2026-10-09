import { useTranslation } from "react-i18next";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { useFoundDevices } from "@/lib/ipc/hardware";
import type { SerialSettings } from "@/lib/ipc/types";

/** pos-hw `serial::BAUDS`: the speeds `hardware_save` accepts for a serial device. */
export const SERIAL_BAUDS = [9_600, 19_200, 38_400, 57_600, 115_200, 4_800, 2_400, 1_200] as const;

/** pos-hw `SerialSettings::eight_n1`: what most scales, displays, scanners and adapters use. */
export function eightN1(port: string, baud = 9_600): SerialSettings {
  return { port, baud, dataBits: "eight", parity: "none", stopBits: "one" };
}

/** The COM ports Windows lists (the hub's discovery), plus the saved one when it is unplugged. */
export function PortSelect({
  id,
  label,
  value,
  onChange,
  none,
  error,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (port: string) => void;
  /** The empty choice ("Aucune balance", "Choisir…"). */
  none: string;
  error?: string;
}) {
  const { t } = useTranslation();
  const found = useFoundDevices();
  const ports = (found.data ?? []).flatMap((d) => (d.kind === "serial_port" ? [d] : []));
  const listed =
    value !== "" && !ports.some((p) => p.port === value)
      ? [{ port: value, description: null }, ...ports]
      : ports;
  return (
    <Field id={id} label={label} error={error}>
      <NativeSelect id={id} dir="ltr" value={value} onChange={(e) => onChange(e.target.value)}>
        <option value="">{none}</option>
        {listed.map((p) => (
          <option key={p.port} value={p.port}>
            {p.description
              ? t("hardware:devices.portOption", { port: p.port, description: p.description })
              : p.port}
          </option>
        ))}
      </NativeSelect>
    </Field>
  );
}

export function BaudSelect({
  id,
  label,
  value,
  onChange,
}: {
  id: string;
  label: string;
  value: number;
  onChange: (baud: number) => void;
}) {
  return (
    <Field id={id} label={label}>
      <NativeSelect id={id} dir="ltr" value={value} onChange={(e) => onChange(Number(e.target.value))}>
        {SERIAL_BAUDS.map((b) => (
          <option key={b} value={b}>
            {b}
          </option>
        ))}
      </NativeSelect>
    </Field>
  );
}
