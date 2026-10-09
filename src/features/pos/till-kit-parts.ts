import { BackupAlerts } from "@/app/BackupAlerts";
import { DemoRibbon } from "@/app/DemoRibbon";
import { LanBanner } from "@/app/LanBanner";
import { backOfficeHome } from "@/app/nav";
import { StatusPills } from "@/app/StatusPills";
import { useHotkeys } from "@/lib/hotkeys";
import { useCart } from "./cart-store";
import { useExtras } from "./extras-store";
import { PaymentDialog } from "./PaymentDialog";
import { ProductGrid } from "./ProductGrid";
import { ScanField } from "./ScanField";
import type { TillKit } from "./till-kit";
import { lastDisplayFrame } from "./use-customer-display";

/** The till's parts as the entry holds them (`till-kit.ts`). */
export const tillKit: TillKit = {
  backOfficeHome,
  BackupAlerts,
  DemoRibbon,
  LanBanner,
  PaymentDialog,
  ProductGrid,
  ScanField,
  StatusPills,
  useHotkeys,
  useCart,
  useExtras,
  lastDisplayFrame,
};
