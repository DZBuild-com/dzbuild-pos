import { cleanup, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { REPORT_PERMISSIONS, backOfficeHome, visibleNavItems } from "@/app/nav";
import { AppRoutes } from "@/app/routes";
import { i18n, type Lang, setLanguage } from "@/lib/i18n";
import type { Permission, ReportInfo, Session } from "@/lib/ipc/types";
import {
  appInfoFixture,
  backupFixture,
  cashierSession,
  companyFixture,
  managerSession,
  ownerSession,
  ownerTile,
  packFixture,
  settingsValues,
  storekeeperSession,
  updateStatusFixture,
} from "@/test/fixtures";
import { cashSessionFixture, categoriesFixture, productHitsFixture } from "@/test/fixtures-m1";
import {
  customerFixture,
  customerHitsFixture,
  dashboardFixture,
  importJobFixture,
  ledgerPageFixture,
  levelsFixture,
  locationsFixture,
  promotionFixture,
  purchaseSummariesFixture,
  reportViewFixture,
  supplierFixture,
} from "@/test/fixtures-m2";
import { lanStandaloneFixture } from "@/test/fixtures-m5";
import { unlinkedStatus } from "@/test/fixtures-m7";
import { packState } from "@/test/fixtures-m8";
import { activeStatus } from "@/test/fixtures-m9";
import { type Handler, missingKeys, mockCommands, renderWithProviders } from "@/test/render";

/** The four built-in roles of the shop. */
const ROLES: Record<string, Session> = {
  owner: ownerSession,
  manager: managerSession,
  cashier: cashierSession,
  storekeeper: storekeeperSession,
};

/** Report kinds the catalogue lists, with the permission of their group (`ReportGroup::permission`)
 * and their titles as the backend (`pos-docs`) sends them. */
const REPORTS: {
  kind: ReportInfo["kind"];
  group: ReportInfo["group"];
  permission: Permission;
  title: Record<Lang, string>;
  groupTitle: Record<Lang, string>;
}[] = [
  {
    kind: "sales_by_product",
    group: "sales",
    permission: "reports.sales",
    title: { ar: "المبيعات حسب المنتج", fr: "Ventes par produit", en: "Sales by product" },
    groupTitle: { ar: "المبيعات", fr: "Ventes", en: "Sales" },
  },
  {
    kind: "stock_levels",
    group: "stock",
    permission: "reports.stock",
    title: { ar: "مستويات المخزون", fr: "Niveaux de stock", en: "Stock levels" },
    groupTitle: { ar: "المخزون", fr: "Stock", en: "Stock" },
  },
  {
    kind: "customer_balances",
    group: "customers",
    permission: "credit.view",
    title: { ar: "أرصدة الزبائن", fr: "Soldes clients", en: "Customer balances" },
    groupTitle: { ar: "الزبائن", fr: "Clients", en: "Customers" },
  },
];

function catalogFor(session: Session): ReportInfo[] {
  return REPORTS.map(({ kind, group, permission, title, groupTitle }) => ({
    kind,
    group,
    title: title[session.lang],
    groupTitle: groupTitle[session.lang],
    filters: [],
    dated: kind === "sales_by_product",
    requiredFilter: null,
    allowed: session.permissions.includes(permission),
  }));
}

/** The commands the M2 screens and the shell call, answered with the M2 fixtures. Returns the
 * calls no fixture answered (such a screen would show an error, not its content). */
function mockShop(session: Session): () => string[] {
  const handlers: Record<string, Handler> = {
    auth_session: () => session,
    company_get: () => companyFixture,
    settings_get: ({ key }) => settingsValues[String(key)],
    datapack_get: () => packFixture,
    app_info: () => appInfoFixture,
    cash_current: () => cashSessionFixture,
    cash_sessions: () => [cashSessionFixture],
    // the shop is not linked to DZBuild: no sync dot, no online orders in the rail
    cloud_status: () => unlinkedStatus,
    // a supérette with no pack module on (M8): no pack screen in the rail
    packs_state: () => packState(),
    backup_status: () => ({
      lastAuto: null,
      newestAt: backupFixture.at,
      stale: false,
      databaseDamaged: false,
    }),
    updates_status: () => updateStatusFixture,
    app_last_crash: () => null,
    support_contact: () => settingsValues["support.contact"],
    demo_status: () => ({ active: false, exists: false, ownerPin: null, cashierPin: null }),
    hardware_get: () => ({
      version: 2,
      printers: [],
      receiptPrinterId: null,
      labelPrinterId: null,
      receipt: {
        printByDefault: true,
        openDrawerOnCash: true,
        drawerBeforePrint: false,
        logo: true,
        secondLang: null,
      },
      scanner: { layout: "us" },
    }),
    reports_dashboard: () => dashboardFixture,
    reports_catalog: () => catalogFor(session),
    reports_run: () => reportViewFixture,
    customers_search: () => customerHitsFixture,
    customers_get: ({ id }) => ({ ...customerFixture, id: String(id) }),
    customers_documents: () => [],
    credit_entries: () => ledgerPageFixture,
    suppliers_search: () => [
      {
        id: supplierFixture.id,
        code: supplierFixture.code,
        name: supplierFixture.name,
        phone: supplierFixture.phone,
        wilaya: supplierFixture.wilaya,
        balance: supplierFixture.balance,
        archived: false,
      },
    ],
    purchases_list: () => purchaseSummariesFixture,
    stock_locations: () => locationsFixture,
    stock_levels_view: () => levelsFixture,
    promotions_list: () => [promotionFixture],
    catalog_categories_list: () => categoriesFixture,
    products_catalog: () => productHitsFixture,
    auth_list_users: () => [ownerTile],
    cash_movements: () => [],
    documents_list: () => [],
    imports_jobs: () => [importJobFixture],
    // M5: the shell's network pill and banner, and the screens that ask whether this PC is a till
    lan_status: () => lanStandaloneFixture,
    // M9: the shell's licence banner (a lifetime licence: no banner)
    license_status: () => activeStatus,
  };
  const calls = mockCommands(handlers);
  return () => calls.filter((command) => !(command in handlers));
}

type Screen = { level: number; name: string | RegExp };

/** The heading each screen shows, in the current language. */
function screenOf(path: string, session: Session): Screen {
  const t = (key: string) => i18n.t(key);
  const h1 = (name: string | RegExp): Screen => ({ level: 1, name });
  switch (path) {
    case "/dashboard":
      return h1(t("nav:dashboard"));
    case "/sales":
      return h1(t("documents:title"));
    case "/products":
      return h1(t("products:title"));
    case "/customers":
      return h1(t("customers:title"));
    case "/customers/c1":
      return h1(new RegExp(customerFixture.name));
    case "/purchases":
      return h1(t("purchases:title"));
    case "/stock":
      return h1(t("stock:title"));
    case "/products/promotions":
      return h1(t("promotions:title"));
    case "/reports":
      return h1(t("reports:title"));
    case "/reports/sales_by_product":
      return session.permissions.includes("reports.sales")
        ? h1(REPORTS[0]!.title[session.lang])
        : { level: 2, name: t("reports:unavailable.title") };
    case "/cash":
      return h1(t("cash:title"));
    case "/imports/products":
      return h1(t("imports:title"));
    case "/settings/sales":
      return { level: 2, name: t("settings:salesRules.payments") };
    case "/settings/data":
      return { level: 2, name: t("settings:data.exportAllTitle") };
    case "/settings/about":
      return { level: 2, name: appInfoFixture.name };
    default:
      throw new Error(`no screen for ${path}`);
  }
}

/** Who may open each route (`AppRoutes`), and where the others land instead. */
const ROUTES: { path: string; anyOf: readonly Permission[]; otherwise?: string }[] = [
  { path: "/dashboard", anyOf: ["reports.sales"] },
  { path: "/customers", anyOf: ["customers.view"] },
  { path: "/customers/c1", anyOf: ["customers.view"] },
  { path: "/purchases", anyOf: ["purchases.view", "suppliers.manage"] },
  { path: "/stock", anyOf: ["stock.view", "stock.count"] },
  { path: "/products/promotions", anyOf: ["products.view"] },
  { path: "/reports", anyOf: REPORT_PERMISSIONS },
  { path: "/reports/sales_by_product", anyOf: REPORT_PERMISSIONS },
  { path: "/cash", anyOf: ["cash.open", "cash.close", "cash.view_history"] },
  {
    path: "/imports/products",
    anyOf: ["products.import", "customers.manage", "suppliers.manage", "stock.count"],
  },
  { path: "/settings/sales", anyOf: ["settings.company"], otherwise: "/settings/about" },
  { path: "/settings/data", anyOf: ["settings.data"], otherwise: "/settings/about" },
];

async function visit(session: Session, path: string): Promise<void> {
  const unanswered = mockShop(session);
  renderWithProviders(<AppRoutes session={session} />, { route: path });
  const route = ROUTES.find((r) => r.path === path)!;
  const allowed = route.anyOf.some((p) => session.permissions.includes(p));
  const landing = allowed ? path : (route.otherwise ?? backOfficeHome(session));
  const expected = screenOf(landing, session);
  // text queries restricted to the heading level: role queries over whole screens are slow in
  // jsdom, and a slow smoke starves the timing-sensitive tests running beside it
  expect(
    await screen.findByText(expected.name, { selector: `h${expected.level}` }),
    `${session.roleNameEn} on ${path}`,
  ).toBeInTheDocument();
  const nav = screen.getByLabelText(i18n.t("nav:mainMenu"), { selector: "nav" });
  const links = Array.from(nav.querySelectorAll("a"), (a) => a.textContent?.trim());
  expect(links, `${session.roleNameEn}: the nav`).toEqual(
    visibleNavItems(session).map((item) => i18n.t(item.labelKey)),
  );
  expect(unanswered(), `${session.roleNameEn} on ${path}: every call answered`).toEqual([]);
  cleanup();
}

describe("M2 screens smoke", () => {
  it("the collector reports a key the language lacks", async () => {
    missingKeys.clear();
    i18n.addResource("fr", "common", "smokeOnlyInFrench", "Seulement en français");
    await setLanguage("ar");
    i18n.t("common:smokeOnlyInFrench");
    i18n.t("common:smokeNowhere");
    await setLanguage("fr");
    i18n.t("common:smokeOnlyInFrench");
    expect([...missingKeys].sort()).toEqual(["ar:common:smokeNowhere", "ar:common:smokeOnlyInFrench"]);
    missingKeys.clear();
  });

  describe.each(["fr", "ar", "en"] satisfies Lang[])("in %s", (lang) => {
    it.each(Object.keys(ROLES))(
      "%s reaches every M2 route or its no-access state",
      async (role) => {
        // jsdom has no layout: the virtual tables show rows once their scroller measures something
        vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
        vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
        const session: Session = { ...ROLES[role]!, lang };
        await setLanguage(lang);
        missingKeys.clear();
        for (const route of ROUTES) {
          await visit(session, route.path);
          expect(document.documentElement.dir, route.path).toBe(lang === "ar" ? "rtl" : "ltr");
        }
        expect([...missingKeys], `${role} in ${lang}`).toEqual([]);
      },
      60_000,
    );
  });
});
