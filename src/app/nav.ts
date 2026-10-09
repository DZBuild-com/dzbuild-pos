import {
  Boxes,
  ChartColumn,
  CircleHelp,
  FileText,
  Globe,
  LayoutDashboard,
  type LucideIcon,
  Package,
  ReceiptText,
  Settings,
  ShoppingCart,
  Truck,
  Users,
  Wallet,
} from "lucide-react";
import { type LazyExoticComponent, lazy } from "react";
import type { Module, Permission, Session } from "@/lib/ipc/types";

/** A pack's rail icon, read with the packs' icons chunk the first time it shows. */
function packIcon(name: keyof typeof import("./pack-icons")): LazyExoticComponent<LucideIcon> {
  return lazy(() => import("./pack-icons").then((icons) => ({ default: icons[name] })));
}

export interface NavItem {
  path: string;
  labelKey: string;
  /** A pack's icon is lazy: the rail shows it inside `Suspense`. */
  icon: LucideIcon | LazyExoticComponent<LucideIcon>;
  /** Shown when the user has any of these; empty = everyone. */
  anyOf: readonly Permission[];
  /** Shown only while a DZBuild store is linked to this PC: the online orders (the link is
   * optional, so an unlinked shop never sees an empty module). */
  linkedOnly?: boolean;
  /** Shown only while one of these modules is on (the vertical packs, M8). */
  modules?: readonly Module[];
}

/** Who reaches the reports: the catalogue then offers the kinds each permission runs. Mirrors
 * `ReportGroup::permission()` (purchases need `purchases.view`, customers `credit.view`) plus the
 * spec's `reports.tax`. */
export const REPORT_PERMISSIONS: readonly Permission[] = [
  "reports.sales",
  "reports.stock",
  "reports.profit",
  "reports.cash",
  "reports.tax",
  "purchases.view",
  "credit.view",
];

/** Who makes commercial documents: any of the four kinds' permissions (`billing::DOCUMENT_PERMISSIONS`). */
export const DOCUMENT_PERMISSIONS: readonly Permission[] = [
  "documents.invoice",
  "documents.quote",
  "documents.delivery",
  "documents.credit_note",
];

/** Who reaches the document archive (`/documents`, `/documents/:id`): the M3 rule's four kinds'
 * permissions or `sales.view_all`, and the supplier documents' `purchases.view` and the transfer
 * notes' `stock.transfer` — every permission of `billing::ARCHIVE_PERMISSIONS` that shows a kind
 * there (`visible_kinds`). Its `sales.reprint` is left out: it only keeps a cashier's call from
 * being refused and shows him no kind (his tickets stay in `/sales`). */
export const ARCHIVE_PERMISSIONS: readonly Permission[] = [
  ...DOCUMENT_PERMISSIONS,
  "sales.view_all",
  "purchases.view",
  "stock.transfer",
];

export const NAV_ITEMS: readonly NavItem[] = [
  { path: "/dashboard", labelKey: "nav:dashboard", icon: LayoutDashboard, anyOf: ["reports.sales"] },
  { path: "/pos", labelKey: "nav:pos", icon: ShoppingCart, anyOf: ["sales.create"] },
  { path: "/orders", labelKey: "nav:orders", icon: Globe, anyOf: ["sales.create"], linkedOnly: true },
  {
    path: "/sales",
    labelKey: "nav:sales",
    icon: ReceiptText,
    anyOf: ["sales.view_all", "sales.reprint", "sales.return"],
  },
  { path: "/cash", labelKey: "nav:cash", icon: Wallet, anyOf: ["cash.open", "cash.view_history"] },
  { path: "/products", labelKey: "nav:products", icon: Package, anyOf: ["products.view"] },
  { path: "/stock", labelKey: "nav:stock", icon: Boxes, anyOf: ["stock.view", "stock.count"] },
  { path: "/customers", labelKey: "nav:customers", icon: Users, anyOf: ["customers.view"] },
  {
    path: "/purchases",
    labelKey: "nav:purchases",
    icon: Truck,
    anyOf: ["purchases.view", "suppliers.manage"],
  },
  { path: "/documents", labelKey: "nav:documents", icon: FileText, anyOf: ARCHIVE_PERMISSIONS },
  { path: "/reports", labelKey: "nav:reports", icon: ChartColumn, anyOf: REPORT_PERMISSIONS },
  { path: "/settings", labelKey: "nav:settings", icon: Settings, anyOf: [] },
  { path: "/help", labelKey: "nav:help", icon: CircleHelp, anyOf: [] },
];

/** Modules with a real screen; the others still route to the "coming soon" page. */
export const LIVE_MODULES: readonly string[] = [
  "/dashboard",
  "/cash",
  "/pos",
  "/orders",
  "/sales",
  "/products",
  "/stock",
  "/customers",
  "/purchases",
  "/documents",
  "/reports",
  "/help",
];

/** Modules of later milestones: routed to the "coming soon" page. */
export const MODULE_ITEMS: readonly NavItem[] = NAV_ITEMS.filter(
  (item) => item.path !== "/settings" && !LIVE_MODULES.includes(item.path),
);

/** The vertical packs' screens (M8): each shows while one of its modules is on. The pharmacy,
 * butcher and variant screens are tabs of `/products` and `/stock`; the customer orders live at
 * `/customer-orders`, since `/orders` is the DZBuild store's online orders (M7). */
export const PACK_ITEMS: readonly NavItem[] = [
  {
    path: "/restaurant",
    labelKey: "nav:restaurant",
    icon: packIcon("ChefHat"),
    anyOf: ["sales.create", "settings.company"],
    modules: ["restaurant"],
  },
  {
    path: "/services",
    labelKey: "nav:services",
    icon: packIcon("Wrench"),
    anyOf: ["sales.create"],
    modules: ["repairs"],
  },
  {
    path: "/instalments",
    labelKey: "nav:instalments",
    icon: packIcon("HandCoins"),
    anyOf: ["credit.view", "sales.create"],
    modules: ["instalments"],
  },
  {
    path: "/rounds",
    labelKey: "nav:rounds",
    icon: packIcon("Van"),
    anyOf: ["documents.delivery"],
    modules: ["delivery_rounds"],
  },
  {
    path: "/customer-orders",
    labelKey: "nav:customerOrders",
    icon: packIcon("CalendarClock"),
    anyOf: ["sales.create", "documents.quote"],
    modules: ["orders", "appointments"],
  },
  {
    path: "/loyalty",
    labelKey: "nav:loyalty",
    icon: packIcon("Heart"),
    // the cards and vouchers are the counter's, the points report `reports.cash`'s, the rules
    // `settings.company`'s
    anyOf: ["customers.view", "sales.create", "reports.cash", "settings.company"],
    modules: ["loyalty", "gift_vouchers"],
  },
  {
    path: "/jewellery",
    labelKey: "nav:jewellery",
    icon: packIcon("Gem"),
    // the register tab needs `customers.manage` (the buyers' identities)
    anyOf: ["products.view", "sales.create", "customers.manage"],
    modules: ["jewellery"],
  },
  {
    path: "/staff",
    labelKey: "nav:staff",
    icon: packIcon("UserCog"),
    // the files and advances are `settings.users`'s, the shifts `reports.cash`'s
    anyOf: ["settings.users", "reports.cash"],
    modules: ["staff"],
  },
  {
    path: "/zakat",
    labelKey: "nav:zakat",
    icon: packIcon("Scale"),
    anyOf: ["reports.profit"],
    modules: ["zakat"],
  },
];

/** The rail's items for this user; the online orders only while a DZBuild store is `linked`. */
export function visibleNavItems(session: Session, linked = false): NavItem[] {
  return NAV_ITEMS.filter((item) => (linked || !item.linkedOnly) && mayOpen(session, item));
}

function mayOpen(session: Session, item: NavItem): boolean {
  return item.anyOf.length === 0 || item.anyOf.some((p) => session.permissions.includes(p));
}

/** The rail: the screens the user may open, then the pack screens whose module is on (and that
 * the user may open), just before the settings. */
export function visibleItems(session: Session, modules: readonly Module[], linked = false): NavItem[] {
  const on = (item: NavItem) => !item.modules || item.modules.some((m) => modules.includes(m));
  const packs = PACK_ITEMS.filter((item) => mayOpen(session, item) && on(item));
  const base = visibleNavItems(session, linked);
  const settingsAt = base.findIndex((item) => item.path === "/settings");
  return [...base.slice(0, settingsAt), ...packs, ...base.slice(settingsAt)];
}

/** The first back-office screen with real content the user may open: the dashboard with
 * `reports.sales`, else the first allowed module (sales history, cash, products, stock…), else the
 * settings; never a "coming soon" page, the help centre, nor the online orders of the optional
 * DZBuild link. */
export function backOfficeHome(session: Session): string {
  const live = visibleNavItems(session).find(
    (item) => item.path !== "/pos" && item.path !== "/help" && LIVE_MODULES.includes(item.path),
  );
  return live?.path ?? "/settings";
}

/** Where a sign-in lands (after an idle lock too): the sale screen for whoever sells, else
 * {@link backOfficeHome}. */
export function landingRoute(session: Session): string {
  return session.permissions.includes("sales.create") ? "/pos" : backOfficeHome(session);
}
