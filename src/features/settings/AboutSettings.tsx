import {
  BookOpen,
  ExternalLink,
  FileText,
  FolderOpen,
  Link2,
  type LucideIcon,
  Stethoscope,
  Store,
} from "lucide-react";
import { Fragment, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { Splash } from "@/components/splash";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { ltrIn } from "@/features/updates/ready";
import { markUrl } from "@/lib/brand";
import { useAppInfo, useOpenFolder, useOpenLink } from "@/lib/ipc/app";
import { useLastCrash, useSupportContact } from "@/lib/ipc/diagnostics";
import type { Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { SupportCard } from "./SupportCard";

export function AboutSettings({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const info = useAppInfo();
  const open = useOpenFolder();
  const crash = useLastCrash();
  // Asked with the page, not once the card mounts after the version: the page shows at once.
  useSupportContact();
  // The folders hold pos.db and its backups: `app_open_folder` requires settings.data, as does
  // the Diagnostic screen.
  const canOpenFolders = session.permissions.includes("settings.data");
  if (!info.data) return <Splash inline />;
  const rows: [string, string][] = [
    ["settings:about.version", info.data.version],
    ["settings:about.terminal", info.data.terminalCode],
    ["settings:about.schema", String(info.data.schemaVersion)],
    ["settings:about.os", info.data.os],
    ["settings:about.dataFolder", info.data.paths.dataDir],
  ];
  return (
    <div className="flex flex-col gap-6">
      <Card>
        <div className="flex items-center gap-3">
          <img src={markUrl} alt="" className="size-12" />
          <h2 className="text-2xl font-bold">{info.data.name}</h2>
        </div>
        <p className="text-ink-2">{t("settings:about.tagline")}</p>
        <p className="font-medium">{t("settings:about.free")}</p>
        <dl className="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-2">
          {rows.map(([key, value]) => (
            <Fragment key={key}>
              <dt className="font-semibold">{t(key)}</dt>
              <dd>
                <bdi dir="ltr" className="break-all">
                  {value}
                </bdi>
              </dd>
            </Fragment>
          ))}
        </dl>
        {crash.data ? (
          <p className="text-warn">{t("settings:about.lastCrash", { at: ltrIn(crash.data.at, lang) })}</p>
        ) : null}
        {canOpenFolders ? (
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" onClick={() => open.mutate("data")}>
              <FolderOpen aria-hidden />
              {t("settings:about.openData")}
            </Button>
            <Button variant="outline" onClick={() => open.mutate("logs")}>
              <FileText aria-hidden />
              {t("settings:about.openLogs")}
            </Button>
            <Link
              to="/settings/diagnostics"
              className="flex min-h-11 items-center gap-2 rounded-control px-3 font-semibold text-info underline hover:bg-surface-2 focus-visible:outline-3 focus-visible:outline-focus"
            >
              <Stethoscope aria-hidden className="size-5 shrink-0" />
              {t("settings:about.openDiagnostics")}
            </Link>
          </div>
        ) : null}
        {open.error ? (
          <p role="alert" className="text-danger">
            {errorText(open.error)}
          </p>
        ) : null}
      </Card>
      <GrowCard session={session} />
      <SupportCard session={session} editable={false} />
    </div>
  );
}

/** The free edition's only word about DZBuild: three quiet rows, each opened on request (a
 * website, the shop's link, the blog). Never a popup, never in the way. The browser opens through
 * `app_open_link`: the webview names a fixed link, never a URL. */
function GrowCard({ session }: { session: Session }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const openLink = useOpenLink();
  // Settings › DZBuild is for `settings.dzbuild` holders only (the route sends others to About).
  const canLink = session.permissions.includes("settings.dzbuild");
  const external = (kind: "dzbuild_register" | "dzbuild_blog", label: string) => (
    <Button variant="outline" disabled={openLink.isPending} onClick={() => openLink.mutate(kind)}>
      <ExternalLink aria-hidden className="rtl:-scale-x-100" />
      {label}
    </Button>
  );
  return (
    <Card aria-labelledby="grow-title">
      <h2 id="grow-title" className="text-xl font-bold">
        {t("settings:about.grow.title")}
      </h2>
      <ul className="flex flex-col divide-y divide-line">
        <GrowRow icon={Store} text={t("settings:about.grow.website.text")}>
          {external("dzbuild_register", t("settings:about.grow.website.action"))}
        </GrowRow>
        {canLink ? (
          <GrowRow icon={Link2} text={t("settings:about.grow.link.text")}>
            <Button asChild variant="outline">
              <Link to="/settings/dzbuild">{t("settings:about.grow.link.action")}</Link>
            </Button>
          </GrowRow>
        ) : null}
        <GrowRow icon={BookOpen} text={t("settings:about.grow.blog.text")}>
          {external("dzbuild_blog", t("settings:about.grow.blog.action"))}
        </GrowRow>
      </ul>
      {openLink.error ? (
        <p role="alert" className="text-danger">
          {errorText(openLink.error)}
        </p>
      ) : null}
    </Card>
  );
}

function GrowRow({ icon: Icon, text, children }: { icon: LucideIcon; text: string; children: ReactNode }) {
  return (
    <li className="flex flex-wrap items-center gap-3 py-3 first:pt-0 last:pb-0">
      <span
        aria-hidden
        className="grid size-10 shrink-0 place-items-center rounded-full bg-brand text-brand-ink"
      >
        <Icon className="size-5" />
      </span>
      <p className="min-w-0 grow basis-60">{text}</p>
      {children}
    </li>
  );
}
