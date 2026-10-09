import { Suspense } from "react";
import { useTranslation } from "react-i18next";
import { NavLink } from "react-router";
import { markUrl } from "@/lib/brand";
import { useCloudStatus } from "@/lib/ipc/cloud";
import { usePackState } from "@/lib/ipc/pack-state";
import type { Session } from "@/lib/ipc/types";
import { cn } from "@/lib/utils";
import { visibleItems } from "./nav";

/** Start-side rail, 96 px; 56 px icons-only under 1100 px (spec §10.5). The online orders show
 * while a DZBuild store is linked, with the count of orders nobody opened yet; a pack's screen
 * while its module is on. */
export function NavRail({ session }: { session: Session }) {
  const { t } = useTranslation();
  const status = useCloudStatus().data;
  const modules = usePackState().data?.modules ?? [];
  const unseen = status?.unseenOrders ?? 0;
  return (
    <nav
      aria-label={t("nav:mainMenu")}
      className="flex w-24 shrink-0 flex-col gap-1 overflow-y-auto border-e border-line bg-surface p-2 max-[1100px]:w-14"
    >
      <img src={markUrl} alt="" className="mx-auto mb-2 size-10" />
      {visibleItems(session, modules, status?.linked === true).map((item) => {
        const label = t(item.labelKey);
        const badge = item.path === "/orders" && unseen > 0;
        return (
          <NavLink
            key={item.path}
            to={item.path}
            title={label}
            aria-label={badge ? `${label} ${unseen}` : undefined}
            className={({ isActive }) =>
              cn(
                "relative flex min-h-14 flex-col items-center justify-center gap-1 rounded-control px-1 text-center text-sm font-semibold",
                isActive ? "bg-brand-soft text-ink" : "text-ink-2 hover:bg-surface-2",
              )
            }
          >
            {/* a pack's icon arrives with its chunk: its place is kept meanwhile */}
            <Suspense fallback={<span aria-hidden className="size-6" />}>
              <item.icon aria-hidden className="size-6" />
            </Suspense>
            <span className="max-[1100px]:sr-only">{label}</span>
            {badge ? (
              <span
                aria-hidden
                className="absolute end-1 top-1 min-w-5 rounded-full bg-danger px-1.5 text-xs text-surface tabular-nums"
              >
                <bdi dir="ltr">{unseen}</bdi>
              </span>
            ) : null}
          </NavLink>
        );
      })}
    </nav>
  );
}
