import { useTranslation } from "react-i18next";
import { useLanStatus } from "@/lib/ipc/lan";
import { cn } from "@/lib/utils";

/** On a till, settings the main PC owns (the scale items) are shown read-only, with the
 * main PC's name; nothing on a single PC or the main PC. */
export function HostOnlyNote({ className }: { className?: string }) {
  const { t } = useTranslation();
  const status = useLanStatus();
  const client = status.data?.role === "client" ? status.data.client : null;
  if (!client) return null;
  return (
    <p role="note" className={cn("rounded-control bg-info-soft p-3 text-info", className)}>
      {t("sync:lan.shell.hostSettings", { name: client.hostName })}
    </p>
  );
}
