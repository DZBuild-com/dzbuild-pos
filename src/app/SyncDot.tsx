import { Cloud } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useCloudStatus, useSyncEvents } from "@/lib/ipc/cloud";
import { cn } from "@/lib/utils";

/** Spec §10.5: absent without a link (the link is optional); green in sync, amber offline or
 * waiting, red when DZBuild refused items or the link must be redone. The name says how many
 * wait. */
export function SyncDot() {
  const { t } = useTranslation();
  useSyncEvents();
  const { data: s } = useCloudStatus();
  if (!s?.linked) return null;
  const [tone, name] =
    s.linkStatus !== "linked"
      ? ["bg-danger", t("sync:dot.relink")]
      : s.failed > 0
        ? ["bg-danger", t("sync:dot.failed", { count: s.failed })]
        : !s.online
          ? ["bg-warn", t("sync:dot.offline", { count: s.pending })]
          : s.pending > 0
            ? ["bg-warn", t("sync:dot.syncedWaiting", { count: s.pending })]
            : ["bg-ok", t("sync:dot.synced")];
  return (
    <span role="img" aria-label={name} title={name} className="inline-flex items-center gap-1 text-ink-2">
      <Cloud aria-hidden className="size-5" />
      <span aria-hidden className={cn("size-2.5 rounded-full", tone)} />
    </span>
  );
}
