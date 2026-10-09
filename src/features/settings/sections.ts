import {
  Blocks,
  DatabaseBackup,
  Download,
  FileText,
  FolderInput,
  Globe,
  Info,
  Keyboard,
  Lock,
  type LucideIcon,
  MonitorCog,
  Network,
  Printer,
  Receipt,
  Scale,
  ShieldCheck,
  SlidersHorizontal,
  Stethoscope,
  Store,
  Users,
  Wallet,
} from "lucide-react";
import type { Permission, Session } from "@/lib/ipc/types";

export interface SettingsSection {
  path: string;
  labelKey: string;
  icon: LucideIcon;
  permission: Permission | null;
}

export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  { path: "shop", labelKey: "settings:sections.shop", icon: Store, permission: "settings.company" },
  // The activity template, the vertical packs' modules and the till screen (M8).
  {
    path: "activity",
    labelKey: "settings:sections.activity",
    icon: Blocks,
    permission: "settings.company",
  },
  {
    path: "documents",
    labelKey: "settings:sections.documents",
    icon: FileText,
    permission: "settings.company",
  },
  {
    path: "sales",
    labelKey: "settings:sections.sales",
    icon: SlidersHorizontal,
    permission: "settings.company",
  },
  // M8 (Task 30): the custom payment types and the BaridPay QR; the till's keys (UX-017)
  {
    path: "payments",
    labelKey: "config:sections.payments",
    icon: Wallet,
    permission: "settings.company",
  },
  { path: "keys", labelKey: "config:sections.keys", icon: Keyboard, permission: "settings.company" },
  {
    path: "expense-categories",
    labelKey: "settings:sections.expenseCategories",
    icon: Receipt,
    permission: "settings.company",
  },
  { path: "users", labelKey: "settings:sections.users", icon: Users, permission: "settings.users" },
  // M8 (Task 30): the DPO and the processing register (PDP-005)
  {
    path: "privacy",
    labelKey: "config:sections.privacy",
    icon: Lock,
    permission: "settings.company",
  },
  {
    path: "hardware",
    labelKey: "settings:sections.hardware",
    icon: Printer,
    permission: "settings.hardware",
  },
  { path: "network", labelKey: "settings:sections.network", icon: Network, permission: "settings.data" },
  // This PC's screen: light mode, text size, on-screen keyboard.
  {
    path: "display",
    labelKey: "settings:sections.display",
    icon: MonitorCog,
    permission: "settings.hardware",
  },
  {
    path: "backups",
    labelKey: "settings:sections.backups",
    icon: DatabaseBackup,
    permission: "settings.data",
  },
  { path: "dzbuild", labelKey: "settings:sections.dzbuild", icon: Globe, permission: "settings.dzbuild" },
  { path: "updates", labelKey: "settings:sections.updates", icon: Download, permission: "settings.data" },
  {
    path: "diagnostics",
    labelKey: "settings:sections.diagnostics",
    icon: Stethoscope,
    permission: "settings.data",
  },
  { path: "data", labelKey: "settings:sections.data", icon: FolderInput, permission: "settings.data" },
  // M8 (Task 30): the signed regulatory packs (PLT-024)
  {
    path: "regulatory",
    labelKey: "config:sections.regulatory",
    icon: Scale,
    permission: "settings.company",
  },
  { path: "audit", labelKey: "settings:sections.audit", icon: ShieldCheck, permission: "audit.view" },
  // M9: this PC's licence (every signed-in user sees it; the owner removes it).
  { path: "about", labelKey: "settings:sections.about", icon: Info, permission: null },
];

export function allowedSections(session: Session): SettingsSection[] {
  return SETTINGS_SECTIONS.filter((s) => s.permission === null || session.permissions.includes(s.permission));
}
