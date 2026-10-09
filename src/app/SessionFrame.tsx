import { useCallback, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Outlet, useNavigate } from "react-router";
import { toast } from "sonner";
import { useLogout } from "@/lib/ipc/auth";
import { useCloudStatus, useSyncEvents } from "@/lib/ipc/cloud";
import { useAutoLockMinutes } from "@/lib/ipc/settings";
import type { Session } from "@/lib/ipc/types";
import { beep } from "@/lib/sounds";
import { useIdleLock } from "@/lib/use-idle-lock";
import { useApplyLanguage } from "@/lib/use-language";
import { DriverPrintFallback } from "./DriverPrintFallback";
import { LanWatch } from "./LanWatch";
import { PrintFailures } from "./PrintFailures";

/** Everything a signed-in screen needs, the full-window sale screen included: the user's
 * language, the idle lock, the toasts of receipts that did not print (and the page print of a
 * receipt on a printer that only takes pages, HW-002), the shop network's status
 * (a till's screens follow the main PC's data) and the DZBuild engine's events (a soft sound when
 * new online orders arrive). */
export function SessionFrame({ session }: { session: Session }) {
  const { t } = useTranslation();
  useApplyLanguage(session.lang);
  const autoLock = useAutoLockMinutes(true);
  const { mutate: logout } = useLogout();
  const lock = useCallback(() => {
    toast(t("auth:login.idleLocked"));
    logout();
  }, [logout, t]);
  useIdleLock(autoLock, lock);
  useNewOrders(session);
  return (
    <>
      <PrintFailures />
      <DriverPrintFallback />
      <LanWatch />
      <Outlet />
    </>
  );
}

/** The engine's events for the whole session; whoever sells hears new online orders arrive (the
 * count already waiting at sign-in is not an arrival). */
function useNewOrders(session: Session): void {
  const { t } = useTranslation();
  const navigate = useNavigate();
  useSyncEvents();
  const unseen = useCloudStatus().data?.unseenOrders;
  const sells = session.permissions.includes("sales.create");
  const previous = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (unseen === undefined) return;
    const before = previous.current;
    previous.current = unseen;
    if (!sells || before === undefined || unseen <= before) return;
    beep("ok");
    toast(t("sync:orders.arrived", { count: unseen }), {
      id: "online-orders-arrived",
      action: { label: t("sync:orders.open"), onClick: () => void navigate("/orders") },
    });
  }, [unseen, sells, t, navigate]);
}
