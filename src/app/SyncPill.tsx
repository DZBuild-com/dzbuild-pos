import { Server, Wifi, WifiOff } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { LINK_TONE } from "@/features/network/link-tone";
import { useLanStatus } from "@/lib/ipc/lan";
import type { Session } from "@/lib/ipc/types";
import { cn } from "@/lib/utils";

const TONE_CLASS = {
  ok: "bg-ok-soft text-ok",
  warn: "bg-warn-soft text-warn",
  danger: "bg-danger-soft text-danger",
  info: "bg-info-soft text-info",
  neutral: "bg-surface-2 text-ink-2",
} as const;

function Pill({
  to,
  tone,
  icon,
  label,
}: {
  to: string | null;
  tone: keyof typeof TONE_CLASS;
  icon: ReactNode;
  label: string;
}) {
  const className = cn(
    "inline-flex min-h-11 items-center gap-2 rounded-full px-3 text-sm font-semibold",
    TONE_CLASS[tone],
  );
  return to ? (
    <Link to={to} className={className} aria-label={label} title={label}>
      {icon}
      <span aria-hidden>{label}</span>
    </Link>
  ) : (
    <span role="status" className={className} aria-label={label} title={label}>
      {icon}
      <span aria-hidden>{label}</span>
    </span>
  );
}

/** UX-024: where this PC stands on the shop's network, one glance from the till. A single PC
 * shows nothing. */
export function SyncPill({ session }: { session: Session }) {
  const { t } = useTranslation();
  const status = useLanStatus();
  const lan = status.data;
  if (!lan || lan.role === "standalone") return null;
  const to = session.permissions.includes("settings.data") ? "/settings/network" : null;
  if (lan.role === "host" && lan.host) {
    const tone = !lan.host.listening ? "danger" : lan.host.noticesOpen > 0 ? "warn" : "ok";
    return (
      <Pill
        to={to}
        tone={tone}
        icon={<Server aria-hidden className="size-4" />}
        label={t("sync:lan.host.tills", { online: lan.host.tillsOnline, total: lan.host.tillsTotal })}
      />
    );
  }
  if (!lan.client) return null;
  const { link, pending } = lan.client;
  const state = t(`sync:lan.link.${link}`);
  const label = pending > 0 ? `${state} · ${t("sync:lan.toSend", { count: pending })}` : state;
  const Icon = link === "offline" ? WifiOff : Wifi;
  return <Pill to={to} tone={LINK_TONE[link]} icon={<Icon aria-hidden className="size-4" />} label={label} />;
}
