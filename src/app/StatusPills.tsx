import { Wallet } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { DeviceDot } from "@/features/settings/hardware/DeviceDot";
import { useCurrentSession } from "@/lib/ipc/cash";
import { useHardwareConfig } from "@/lib/ipc/hardware";
import type { Session } from "@/lib/ipc/types";
import { cn } from "@/lib/utils";
import { PrinterDot, UnprintedSheet } from "./PrintFailures";
import { SyncDot } from "./SyncDot";
import { SyncPill } from "./SyncPill";
import { UpdatePill } from "./UpdatePill";

/** Top bar (spec §10.5): the cash session pill, the printer dot (the receipts to reprint), the
 * shop network's pill on a main PC or a till, the DZBuild sync dot when the shop is linked, the
 * "update ready" pill (PLT-011) and, when this PC has one, the scale and the pole display dots
 * (UX-024). */
export function StatusPills({ session }: { session: Session }) {
  const { t } = useTranslation();
  const till = useCurrentSession();
  const hardware = useHardwareConfig();
  const [listing, setListing] = useState(false);
  const open = till.data !== null && till.data !== undefined;
  return (
    <div className="flex items-center gap-2">
      {till.isSuccess ? (
        <Link
          to="/cash"
          className={cn(
            "inline-flex min-h-11 items-center gap-2 rounded-full px-3 text-sm font-semibold",
            open ? "bg-ok-soft text-ok" : "bg-warn-soft text-warn",
          )}
        >
          <Wallet aria-hidden className="size-4" />
          {open ? t("cash:status.open") : t("cash:status.closed")}
        </Link>
      ) : null}
      {hardware.isSuccess ? (
        <PrinterDot configured={Boolean(hardware.data.receiptPrinterId)} onOpen={() => setListing(true)} />
      ) : null}
      <SyncPill session={session} />
      <SyncDot />
      <UpdatePill session={session} />
      {hardware.data?.scale ? <DeviceDot device="scale" /> : null}
      {hardware.data?.display?.vfd ? <DeviceDot device="vfd" /> : null}
      {listing ? <UnprintedSheet onClose={() => setListing(false)} /> : null}
    </div>
  );
}
