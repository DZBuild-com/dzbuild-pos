import { createContext, useContext } from "react";
import type { BackupAlerts } from "@/app/BackupAlerts";
import type { DemoRibbon } from "@/app/DemoRibbon";
import type { LanBanner } from "@/app/LanBanner";
import type { backOfficeHome } from "@/app/nav";
import type { StatusPills } from "@/app/StatusPills";
import type { useHotkeys } from "@/lib/hotkeys";
import type { useCart } from "./cart-store";
import type { useExtras } from "./extras-store";
import type { PaymentDialog } from "./PaymentDialog";
import type { ProductGrid } from "./ProductGrid";
import type { ScanField } from "./ScanField";
import type { lastDisplayFrame } from "./use-customer-display";

/** The till's own parts that its pack screens use (the restaurant's floor and table ticket, the
 * sale screen's pack extensions).
 * A pack screen is a lazy chunk; importing these from it would make the bundler move
 * them out of the /pos chunk into chunks shared with it (+7 kB gzipped on /pos). The entry hands
 * them over through this context instead (`till-kit-parts.ts`, provided by `AppRoutes`). */
export interface TillKit {
  /** Where the floor's menu goes for the back office. */
  backOfficeHome: typeof backOfficeHome;
  BackupAlerts: typeof BackupAlerts;
  DemoRibbon: typeof DemoRibbon;
  LanBanner: typeof LanBanner;
  PaymentDialog: typeof PaymentDialog;
  ProductGrid: typeof ProductGrid;
  ScanField: typeof ScanField;
  StatusPills: typeof StatusPills;
  useHotkeys: typeof useHotkeys;
  /** The sale's cart and its pack extras (Task 24). */
  useCart: typeof useCart;
  useExtras: typeof useExtras;
  /** The frame the customer screen shows now (the BaridPay QR is shown over it). */
  lastDisplayFrame: typeof lastDisplayFrame;
}

export const TillKitContext = createContext<TillKit | null>(null);

/** The till's parts, inside `AppRoutes` (or a test's `renderM8`). */
export function useTillKit(): TillKit {
  const kit = useContext(TillKitContext);
  if (!kit) throw new Error("the till's parts are provided by AppRoutes");
  return kit;
}
