import { lazy, type ReactNode, Suspense } from "react";
import { loadBackOffice } from "@/lib/i18n";
import { useLicenseStatus } from "@/lib/ipc/license";
import type { Session } from "@/lib/ipc/types";

const PosLicenseCard = lazy(() =>
  Promise.all([import("./PosLicenseCard"), loadBackOffice()]).then(([m]) => ({ default: m.PosLicenseCard })),
);

/** The till sells only with a licence that allows writes; otherwise a read-only card that
 * says why and leads to the back office, whose reports, exports and backups still work. While the
 * status is unknown the till shows (Rust refuses the sale anyway). */
export function PosLicenseGate({ session, children }: { session: Session; children: ReactNode }) {
  const status = useLicenseStatus().data;
  if (!status || status.canWrite) return children;
  return (
    <Suspense fallback={null}>
      <PosLicenseCard session={session} status={status} />
    </Suspense>
  );
}
