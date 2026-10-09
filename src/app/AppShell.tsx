import { Suspense } from "react";
import { useTranslation } from "react-i18next";
import { Outlet } from "react-router";
import { Splash } from "@/components/splash";
import { HelpHotkey } from "@/features/help/HelpHotkey";
import { LicenseBanner } from "@/features/license/LicenseBanner";
import { UpdateReadyDialog } from "@/features/updates/UpdateReadyDialog";
import type { Session } from "@/lib/ipc/types";
import { BackupAlerts } from "./BackupAlerts";
import { DemoRibbon } from "./DemoRibbon";
import { LanBanner } from "./LanBanner";
import { NavRail } from "./NavRail";
import { MainScroll } from "./ScrollToTop";
import { TopBar } from "./TopBar";

export function AppShell({ session }: { session: Session }) {
  const { t } = useTranslation();
  return (
    <div className="flex h-dvh bg-bg text-ink">
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:absolute focus:start-2 focus:top-2 focus:z-50 focus:rounded-control focus:bg-surface focus:p-2"
      >
        {t("nav:skipToContent")}
      </a>
      <NavRail session={session} />
      <div className="flex min-w-0 flex-1 flex-col">
        <DemoRibbon />
        <LicenseBanner />
        <TopBar session={session} />
        <BackupAlerts session={session} />
        <LanBanner />
        <UpdateReadyDialog session={session} />
        <HelpHotkey />
        {/* back to the top on every page change, and the floating "back to top" button */}
        <MainScroll />
        {/* focusable (tabIndex -1) for the skip link and the "back to top" button, never in the Tab order */}
        <main
          id="main"
          tabIndex={-1}
          className="min-h-0 flex-1 overflow-y-auto pb-[var(--osk-inset,0px)] outline-none"
        >
          <div className="max-w-[1280px] px-6 py-6">
            {/* A back-office screen loads its chunk on first use: the rail and the top bar stay. */}
            <Suspense fallback={<Splash inline />}>
              <Outlet />
            </Suspense>
          </div>
        </main>
      </div>
    </div>
  );
}
