export type HelpGroup = "start" | "sell" | "manage" | "settings" | "safety";

export interface HelpTopic {
  id: string;
  group: HelpGroup;
  routes: readonly string[];
  /** Pages of a screen kept in its `?tab=` (M8 pack pages): `"/products?tab=chifa"`. */
  tabs?: readonly string[];
}

/** One page per screen, each in `src/help/{ar,fr,en}/<id>.md` (NFR-027). A new screen adds its row
 * and three pages. */
export const HELP_TOPICS: readonly HelpTopic[] = [
  { id: "first-day", group: "start", routes: ["/help"] },
  { id: "tour", group: "start", routes: [] },
  { id: "quick-card", group: "start", routes: [] },
  { id: "demo", group: "start", routes: [] },
  { id: "pos", group: "sell", routes: ["/pos"] },
  { id: "cash", group: "sell", routes: ["/cash"] },
  { id: "sales", group: "sell", routes: ["/sales"] },
  { id: "orders", group: "sell", routes: ["/orders"] },
  // M8: repairs and services (the cash page's Flexy tab is in `cash`)
  { id: "services", group: "sell", routes: ["/services"] },
  // M8: instalment sales, customer orders and the appointment book
  { id: "instalments", group: "sell", routes: ["/instalments"] },
  { id: "customer-orders", group: "sell", routes: ["/customer-orders"] },
  // M8: delivery rounds, with the customer prices, minimum orders, packs and BL valant facture
  { id: "rounds", group: "sell", routes: ["/rounds"] },
  // M8: loyalty and vouchers
  { id: "loyalty", group: "sell", routes: ["/loyalty"] },
  // M8 restaurant: the floor and the table tickets, then their editors
  { id: "restaurant", group: "sell", routes: ["/pos/floor", "/pos/ticket"] },
  { id: "products", group: "manage", routes: ["/products", "/imports"] },
  { id: "labels", group: "manage", routes: ["/products/labels"] },
  { id: "stock", group: "manage", routes: ["/stock"] },
  { id: "customers", group: "manage", routes: ["/customers"] },
  { id: "purchases", group: "manage", routes: ["/purchases"] },
  { id: "documents", group: "manage", routes: ["/documents"] },
  { id: "dashboard", group: "manage", routes: ["/reports", "/dashboard"] },
  { id: "fiscal", group: "manage", routes: ["/reports/fiscal"] },
  { id: "jewellery", group: "manage", routes: ["/jewellery"] },
  { id: "staff", group: "manage", routes: ["/staff"] },
  { id: "zakat", group: "manage", routes: ["/zakat"] },
  { id: "restaurant-admin", group: "manage", routes: ["/restaurant"] },
  // M8 pharmacy, butcher and catalogue modules: pages of the products and stock screens
  { id: "pharmacy", group: "manage", routes: [], tabs: ["/products?tab=chifa", "/products?tab=register"] },
  { id: "butcher", group: "manage", routes: [], tabs: ["/stock?tab=yield", "/stock?tab=waste"] },
  {
    id: "catalogue-modules",
    group: "manage",
    routes: [],
    tabs: ["/products?tab=regulated", "/products?tab=departments"],
  },
  { id: "settings-shop", group: "settings", routes: ["/settings/shop"] },
  { id: "settings-activity", group: "settings", routes: ["/settings/activity"] },
  { id: "settings-documents", group: "settings", routes: ["/settings/documents"] },
  { id: "settings-sales", group: "settings", routes: ["/settings/sales"] },
  { id: "settings-payments", group: "settings", routes: ["/settings/payments"] },
  { id: "settings-keys", group: "settings", routes: ["/settings/keys"] },
  { id: "settings-expense-categories", group: "settings", routes: ["/settings/expense-categories"] },
  { id: "settings-users", group: "settings", routes: ["/settings/users"] },
  { id: "settings-privacy", group: "settings", routes: ["/settings/privacy"] },
  { id: "settings-hardware", group: "settings", routes: ["/settings/hardware"] },
  { id: "settings-display", group: "settings", routes: ["/settings/display"] },
  { id: "settings-network", group: "settings", routes: ["/settings/network"] },
  { id: "settings-dzbuild", group: "settings", routes: ["/settings/dzbuild"] },
  { id: "settings-regulatory", group: "settings", routes: ["/settings/regulatory"] },
  { id: "settings-audit", group: "settings", routes: ["/settings/audit"] },
  { id: "settings-about", group: "settings", routes: ["/settings/about"] },
  { id: "settings-backups", group: "safety", routes: ["/settings/backups", "/settings/data"] },
  { id: "restore", group: "safety", routes: [] },
  { id: "settings-updates", group: "safety", routes: ["/settings/updates"] },
  { id: "settings-diagnostics", group: "safety", routes: ["/settings/diagnostics"] },
  { id: "move-pc", group: "safety", routes: [] },
  { id: "antivirus", group: "safety", routes: [] },
  { id: "remove-data", group: "safety", routes: [] },
];

export const HELP_GROUPS: readonly HelpGroup[] = ["start", "sell", "manage", "settings", "safety"];

export function isHelpTopic(id: string): boolean {
  return HELP_TOPICS.some((topic) => topic.id === id);
}

/** The topic of a screen: the page of its `?tab=` that has its own topic, else its exact route,
 * else the longest route that prefixes it. */
export function helpTopicFor(pathname: string, search = ""): string {
  const tab = new URLSearchParams(search).get("tab");
  const page =
    tab === null ? null : HELP_TOPICS.find((topic) => topic.tabs?.includes(`${pathname}?tab=${tab}`));
  if (page) return page.id;
  let best: { id: string; length: number } | null = null;
  for (const topic of HELP_TOPICS) {
    for (const route of topic.routes) {
      const matches = pathname === route || pathname.startsWith(`${route}/`);
      if (matches && (!best || route.length > best.length)) best = { id: topic.id, length: route.length };
    }
  }
  return best?.id ?? "first-day";
}

export const helpPath = (pathname: string, search = "") => `/help/${helpTopicFor(pathname, search)}`;
