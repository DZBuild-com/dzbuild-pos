import { Globe } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  useCloudNotices,
  useCloudStatus,
  useFailedItems,
  useOpenCloudPage,
  useSyncEvents,
} from "@/lib/ipc/cloud";
import { useDemoStatus } from "@/lib/ipc/demo";
import type { Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { DevicesPanel } from "./dzbuild/DevicesPanel";
import { FeatureSwitches } from "./dzbuild/FeatureSwitches";
import { LinkWizard } from "./dzbuild/LinkWizard";
import { PublishPanel } from "./dzbuild/PublishPanel";
import { SyncPanel } from "./dzbuild/SyncPanel";

/** Settings › DZBuild (spec §4.7, §10): optional, explained, owner-driven. On a LAN till the link
 * and its settings are the main PC's: the page names it and offers nothing. */
export function DzbuildSettings({ session }: { session: Session }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  useSyncEvents();
  const status = useCloudStatus();
  const open = useOpenCloudPage();
  const linked = status.data?.linked === true;
  // DZBuild asked for a new sign-in, or removed this till: the same store is linked again (the
  // link, its waiting uploads and, after a refused refresh, the device are kept)
  const relink = status.data?.linkStatus === "relink_required" || status.data?.linkStatus === "revoked";
  const mainPc = status.data?.mainPc ?? null;
  // The demo shop never links a DZBuild store (its catalogue would reach the real one).
  const demo = useDemoStatus().data?.active === true;
  const onMainPc = mainPc === null && !demo;
  // read with the status, not after it: the sync card shows them as soon as it appears
  useCloudNotices(onMainPc);
  useFailedItems(onMainPc);
  return (
    <div className="flex flex-col gap-6">
      <Card>
        <h2 className="flex items-center gap-2 text-xl font-bold">
          <Globe aria-hidden className="size-5" />
          {t("sync:section")}
        </h2>
        <p className="text-ink-2">{t("sync:intro")}</p>
        {demo ? (
          <p role="note" className="rounded-control bg-info-soft p-3">
            {t("sync:demo")}
          </p>
        ) : onMainPc ? null : (
          <p role="note" className="rounded-control bg-info-soft p-3">
            {t("sync:till", { host: mainPc })}
          </p>
        )}
        {linked || !onMainPc ? null : (
          <div className="flex flex-wrap items-center gap-3 rounded-control bg-surface-2 p-4">
            <p className="grow text-ink-2">{t("sync:nudge.body")}</p>
            <Button variant="outline" disabled={open.isPending} onClick={() => open.mutate("signup")}>
              {t("sync:nudge.signup")}
            </Button>
          </div>
        )}
        {status.error ? (
          <p role="alert" className="text-danger">
            {errorText(status.error)}
          </p>
        ) : null}
      </Card>
      {status.isSuccess && onMainPc && !linked ? <LinkWizard session={session} /> : null}
      {status.data && onMainPc && linked ? (
        <>
          <SyncPanel status={status.data} session={session} />
          {relink ? <LinkWizard session={session} relink /> : null}
          <FeatureSwitches />
          <PublishPanel session={session} />
          <DevicesPanel session={session} />
        </>
      ) : null}
    </div>
  );
}
