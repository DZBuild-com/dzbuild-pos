import { lazy, Suspense } from "react";
import { loadBackOffice } from "@/lib/i18n";
import { useLicenseStatus } from "@/lib/ipc/license";

const LicenseNotice = lazy(() =>
  Promise.all([import("./LicenseNotice"), loadBackOffice()]).then(([m]) => ({ default: m.LicenseNotice })),
);

/** The licence on every back-office screen and on the sign-in screen: read-only mode,
 * why, and what still works; or a trial's last 3 days. Nothing while the licence allows writes and
 * is far from its end, in the demo shop, or when the status cannot be read (Rust refuses writes
 * anyway). The words load with their own chunk only when there is something to say. */
export function LicenseBanner({ withLink = true }: { withLink?: boolean }) {
  const status = useLicenseStatus().data;
  if (!status || status.state === "demo" || (status.canWrite && !status.warn)) return null;
  return (
    <Suspense fallback={null}>
      <LicenseNotice status={status} withLink={withLink} />
    </Suspense>
  );
}
