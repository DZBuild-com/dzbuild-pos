import { type LucideIcon, MonitorSmartphone, Scale, ScanBarcode } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useDeviceStatus } from "@/lib/ipc/devices";
import type { DeviceKind, DeviceState } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { cn } from "@/lib/utils";

const ICONS: Record<DeviceKind, LucideIcon> = {
  scale: Scale,
  vfd: MonitorSmartphone,
  serial_scanner: ScanBarcode,
};

const OFF: DeviceState = { state: "off" };

/** A scale, pole display or serial scanner at a glance (UX-024): `--ok` connected, `--danger`
 * unplugged or silent (the reason in its label), `--line` not set up on this PC. */
export function DeviceDot({ device }: { device: DeviceKind }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const status = useDeviceStatus();
  const state = status.data?.find((d) => d.device === device)?.state ?? OFF;
  const text =
    state.state === "connected"
      ? t("hardware:devices.connected", { port: state.port })
      : state.state === "disconnected"
        ? t("hardware:devices.disconnected", { port: state.port, reason: errorText({ code: state.code }) })
        : t("hardware:devices.off");
  const tone =
    state.state === "connected" ? "bg-ok" : state.state === "disconnected" ? "bg-danger" : "bg-line";
  const label = t("hardware:devices.label", { device: t(`hardware:devices.names.${device}`), state: text });
  const Icon = ICONS[device];
  return (
    <span role="img" aria-label={label} title={label} className="inline-flex items-center gap-1 text-ink-2">
      <Icon aria-hidden className="size-5" />
      <span aria-hidden className={cn("size-2.5 rounded-full", tone)} />
    </span>
  );
}
