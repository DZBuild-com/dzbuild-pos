import { Suspense, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { NavLink, Outlet, useLocation } from "react-router";
import { Splash } from "@/components/splash";
import type { Session } from "@/lib/ipc/types";
import { cn } from "@/lib/utils";
import { allowedSections } from "./sections";

export function SettingsLayout({ session }: { session: Session }) {
  const { t } = useTranslation();
  const { pathname } = useLocation();
  const nav = useRef<HTMLElement>(null);
  // The sections menu scrolls on its own when it overflows (a row on a narrow window): the open
  // section stays in view. Only then: a menu that fits never scrolls the page under it.
  useEffect(() => {
    const menu = nav.current;
    if (!menu || (menu.scrollWidth <= menu.clientWidth && menu.scrollHeight <= menu.clientHeight)) return;
    const active = menu.querySelector<HTMLElement>('[aria-current="page"]');
    active?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [pathname]);
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-3xl font-bold">{t("settings:title")}</h1>
      <div className="flex flex-col gap-6 lg:flex-row">
        <nav
          ref={nav}
          aria-label={t("settings:title")}
          className="flex shrink-0 gap-1 overflow-x-auto lg:w-56 lg:flex-col"
        >
          {allowedSections(session).map((section) => (
            <NavLink
              key={section.path}
              to={section.path}
              className={({ isActive }) =>
                cn(
                  "flex min-h-11 items-center gap-2 rounded-control px-3 font-semibold whitespace-nowrap",
                  isActive ? "bg-brand-soft text-ink" : "text-ink-2 hover:bg-surface-2",
                )
              }
            >
              <section.icon aria-hidden className="size-5" />
              {t(section.labelKey)}
            </NavLink>
          ))}
        </nav>
        <div className="min-w-0 flex-1">
          {/* Each section is its own chunk: the sections menu stays while one loads. */}
          <Suspense fallback={<Splash inline />}>
            <Outlet />
          </Suspense>
        </div>
      </div>
    </div>
  );
}
