import { Clock, TriangleAlert, WifiOff } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useLanStatus } from "@/lib/ipc/lan";
import { useErrorText } from "@/lib/use-error-text";

/** `sync::SKEW_WARN_MS`. */
const SKEW_WARN_MS = 5 * 60_000;

/** LAN-003: the till keeps selling offline and says so; its clock is checked against the main PC. */
export function LanBanner() {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const status = useLanStatus();
  const client = status.data?.role === "client" ? status.data.client : null;
  if (!client) return null;
  const message =
    client.link === "offline"
      ? t("sync:lan.banner.offline", { count: client.pending })
      : client.link === "attention"
        ? // a refused operation, or why the till cannot sync at all (its token, the vault…)
          client.rejected === 0 && client.lastError
          ? errorText({ code: client.lastError })
          : t("sync:lan.banner.attention")
        : client.link === "version_mismatch"
          ? t("sync:lan.banner.version")
          : null;
  const skewMinutes =
    client.skewMs !== null && Math.abs(client.skewMs) > SKEW_WARN_MS
      ? Math.round(Math.abs(client.skewMs) / 60_000)
      : null;
  // Entries the main PC refused (a register entry, an IMEI, a voucher…) wait on this till
  const refusedCount = client.entriesRefused ?? 0;
  const refused = refusedCount > 0 ? t("sync:lan.banner.refused", { count: refusedCount }) : null;
  // final review (PLT-024): this till's regulatory pack against the main PC's
  const gap = client.datapackGap ?? null;
  const datapack =
    gap === "restart" || gap === "behind" || gap === "ahead" ? t(`sync:lan.banner.datapack.${gap}`) : null;
  if (message === null && skewMinutes === null && refused === null && datapack === null) return null;
  return (
    <div className="flex flex-col gap-1 border-b border-warn bg-warn-soft px-4 py-2 text-ink">
      {message !== null ? (
        <p role="status" className="flex items-center gap-2 font-semibold">
          <WifiOff aria-hidden className="size-5 shrink-0 text-warn" />
          {message}
        </p>
      ) : null}
      {refused !== null ? (
        <p role="alert" className="flex items-center gap-2 font-semibold">
          <TriangleAlert aria-hidden className="size-5 shrink-0 text-warn" />
          {refused}
        </p>
      ) : null}
      {datapack !== null ? (
        <p role="alert" className="flex items-center gap-2 font-semibold">
          <TriangleAlert aria-hidden className="size-5 shrink-0 text-warn" />
          {datapack}
        </p>
      ) : null}
      {skewMinutes !== null ? (
        <p role="alert" className="flex items-center gap-2">
          <Clock aria-hidden className="size-5 shrink-0 text-warn" />
          {t("sync:lan.shell.clockWarning", { minutes: skewMinutes })}
        </p>
      ) : null}
    </div>
  );
}
