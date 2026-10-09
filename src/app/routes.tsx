import { type ReactElement, lazy } from "react";
import { Navigate, Route, Routes } from "react-router";
import { ComingSoon } from "@/features/common/ComingSoon";
import { PosLicenseGate } from "@/features/license/PosLicenseGate";
import { TillKitContext } from "@/features/pos/till-kit";
import { tillKit } from "@/features/pos/till-kit-parts";
import { TillEntry } from "@/features/pos/TillEntry";
import { allowedSections } from "@/features/settings/sections";
import { loadBackOffice } from "@/lib/i18n";
import type { Permission, Session } from "@/lib/ipc/types";
import { AppShell } from "./AppShell";
import {
  ARCHIVE_PERMISSIONS,
  DOCUMENT_PERMISSIONS,
  MODULE_ITEMS,
  PACK_ITEMS,
  REPORT_PERMISSIONS,
  backOfficeHome,
  landingRoute,
} from "./nav";
import { SessionFrame } from "./SessionFrame";

// NFR-010: the till (`/pos`) is in the entry and paints at once; every back-office screen is its
// own chunk, loaded when it is opened (`scripts/perf/bundle-size.mjs` checks both), with the
// back office's texts in the UI language (finding 28: they are not in the till's chunk).
function withTexts<T>(screen: Promise<T>): Promise<T> {
  return Promise.all([screen, loadBackOffice()]).then(([module]) => module);
}
const CashScreen = lazy(() =>
  withTexts(import("@/features/cash/CashScreen")).then((m) => ({ default: m.CashScreen })),
);
const CustomerDetail = lazy(() =>
  withTexts(import("@/features/customers/CustomerDetail")).then((m) => ({ default: m.CustomerDetail })),
);
const CustomersPage = lazy(() =>
  withTexts(import("@/features/customers/CustomersPage")).then((m) => ({ default: m.CustomersPage })),
);
const CustomerOrdersScreen = lazy(() =>
  withTexts(import("@/features/customer-orders/CustomerOrdersScreen")).then((m) => ({
    default: m.CustomerOrdersScreen,
  })),
);
const DashboardPage = lazy(() =>
  withTexts(import("@/features/dashboard/DashboardPage")).then((m) => ({ default: m.DashboardPage })),
);
const DocumentRoute = lazy(() =>
  withTexts(import("@/features/documents/DocumentView")).then((m) => ({ default: m.DocumentRoute })),
);
const DocumentsPage = lazy(() =>
  withTexts(import("@/features/documents/DocumentsPage")).then((m) => ({ default: m.DocumentsPage })),
);
const NewDocumentRoute = lazy(() =>
  withTexts(import("@/features/documents/editor/DocumentEditor")).then((m) => ({
    default: m.NewDocumentRoute,
  })),
);
const SalesHistory = lazy(() =>
  withTexts(import("@/features/documents/SalesHistory")).then((m) => ({ default: m.SalesHistory })),
);
const FiscalPage = lazy(() =>
  withTexts(import("@/features/fiscal/FiscalPage")).then((m) => ({ default: m.FiscalPage })),
);
const HelpCentre = lazy(() =>
  withTexts(import("@/features/help/HelpCentre")).then((m) => ({ default: m.HelpCentre })),
);
const HelpRoute = lazy(() =>
  withTexts(import("@/features/help/HelpCentre")).then((m) => ({ default: m.HelpRoute })),
);
const ImportPage = lazy(() =>
  withTexts(import("@/features/imports/ImportWizard")).then((m) => ({ default: m.ImportPage })),
);
const InstalmentsScreen = lazy(() =>
  withTexts(import("@/features/instalments/InstalmentsScreen")).then((m) => ({
    default: m.InstalmentsScreen,
  })),
);
const JewelleryScreen = lazy(() =>
  withTexts(import("@/features/jewellery/JewelleryScreen")).then((m) => ({ default: m.JewelleryScreen })),
);
const LabelsPage = lazy(() =>
  withTexts(import("@/features/labels/LabelsPage")).then((m) => ({ default: m.LabelsPage })),
);
const LoyaltyScreen = lazy(() =>
  withTexts(import("@/features/loyalty/LoyaltyScreen")).then((m) => ({ default: m.LoyaltyScreen })),
);
const NetworkSettings = lazy(() =>
  withTexts(import("@/features/network/NetworkSettings")).then((m) => ({ default: m.NetworkSettings })),
);
const OrdersScreen = lazy(() =>
  withTexts(import("@/features/orders/OrdersScreen")).then((m) => ({ default: m.OrdersScreen })),
);
// A pack screen and its gate are chunks of their own, with the packs' texts (the back
// office's chunk), never in the till's entry.
const PackGate = lazy(() =>
  withTexts(import("@/features/packs/PackGate")).then((m) => ({ default: m.PackGate })),
);
// M8 restaurant: the floor and the table ticket are the till's pack screens, chunks of
// their own loaded only in a restaurant
const FloorScreen = lazy(() =>
  withTexts(import("@/features/restaurant/FloorScreen")).then((m) => ({ default: m.FloorScreen })),
);
const TicketRoute = lazy(() =>
  withTexts(import("@/features/restaurant/TicketScreen")).then((m) => ({ default: m.TicketRoute })),
);
const RestaurantAdmin = lazy(() =>
  withTexts(import("@/features/restaurant/RestaurantAdmin")).then((m) => ({ default: m.RestaurantAdmin })),
);
const ProductsScreen = lazy(() =>
  withTexts(import("@/features/products/ProductsScreen")).then((m) => ({ default: m.ProductsScreen })),
);
const PromotionsPage = lazy(() =>
  withTexts(import("@/features/promotions/PromotionsPage")).then((m) => ({ default: m.PromotionsPage })),
);
const OrderEditor = lazy(() =>
  withTexts(import("@/features/purchases/OrderEditor")).then((m) => ({ default: m.OrderEditor })),
);
const PurchasesPage = lazy(() =>
  withTexts(import("@/features/purchases/PurchasesPage")).then((m) => ({ default: m.PurchasesPage })),
);
const SupplierDetail = lazy(() =>
  withTexts(import("@/features/purchases/SupplierDetail")).then((m) => ({ default: m.SupplierDetail })),
);
const ReportView = lazy(() =>
  withTexts(import("@/features/reports/ReportView")).then((m) => ({ default: m.ReportView })),
);
const ReportsPage = lazy(() =>
  withTexts(import("@/features/reports/ReportsPage")).then((m) => ({ default: m.ReportsPage })),
);
const AboutSettings = lazy(() =>
  withTexts(import("@/features/settings/AboutSettings")).then((m) => ({ default: m.AboutSettings })),
);
const ActivitySettings = lazy(() =>
  withTexts(import("@/features/settings/ActivitySettings")).then((m) => ({ default: m.ActivitySettings })),
);
const AuditSettings = lazy(() =>
  withTexts(import("@/features/settings/AuditSettings")).then((m) => ({ default: m.AuditSettings })),
);
const BackupsSettings = lazy(() =>
  withTexts(import("@/features/settings/BackupsSettings")).then((m) => ({ default: m.BackupsSettings })),
);
const DataSettings = lazy(() =>
  withTexts(import("@/features/settings/DataSettings")).then((m) => ({ default: m.DataSettings })),
);
const DiagnosticsSettings = lazy(() =>
  withTexts(import("@/features/settings/DiagnosticsSettings")).then((m) => ({
    default: m.DiagnosticsSettings,
  })),
);
const DisplaySettings = lazy(() =>
  withTexts(import("@/features/settings/DisplaySettings")).then((m) => ({ default: m.DisplaySettings })),
);
const DocumentsSettings = lazy(() =>
  withTexts(import("@/features/settings/DocumentsSettings")).then((m) => ({ default: m.DocumentsSettings })),
);
const DzbuildSettings = lazy(() =>
  withTexts(import("@/features/settings/DzbuildSettings")).then((m) => ({ default: m.DzbuildSettings })),
);
const ExpenseCategoriesSettings = lazy(() =>
  withTexts(import("@/features/settings/ExpenseCategoriesSettings")).then((m) => ({
    default: m.ExpenseCategoriesSettings,
  })),
);
const HardwareHub = lazy(() =>
  withTexts(import("@/features/settings/hardware/HardwareHub")).then((m) => ({ default: m.HardwareHub })),
);
const SalesRulesSettings = lazy(() =>
  withTexts(import("@/features/settings/SalesRulesSettings")).then((m) => ({
    default: m.SalesRulesSettings,
  })),
);
const RoundsScreen = lazy(() =>
  withTexts(import("@/features/wholesale/RoundsScreen")).then((m) => ({ default: m.RoundsScreen })),
);
const ServicesScreen = lazy(() =>
  withTexts(import("@/features/services/ServicesScreen")).then((m) => ({ default: m.ServicesScreen })),
);
const SettingsLayout = lazy(() =>
  withTexts(import("@/features/settings/SettingsLayout")).then((m) => ({ default: m.SettingsLayout })),
);
const ShopSettings = lazy(() =>
  withTexts(import("@/features/settings/ShopSettings")).then((m) => ({ default: m.ShopSettings })),
);
const UpdatesSettings = lazy(() =>
  withTexts(import("@/features/settings/UpdatesSettings")).then((m) => ({ default: m.UpdatesSettings })),
);
const UsersSettings = lazy(() =>
  withTexts(import("@/features/settings/UsersSettings")).then((m) => ({ default: m.UsersSettings })),
);
const KeysSettings = lazy(() =>
  withTexts(import("@/features/settings/KeysSettings")).then((m) => ({ default: m.KeysSettings })),
);
const PaymentsSettings = lazy(() =>
  withTexts(import("@/features/settings/PaymentsSettings")).then((m) => ({ default: m.PaymentsSettings })),
);
const PrivacySettings = lazy(() =>
  withTexts(import("@/features/settings/PrivacySettings")).then((m) => ({ default: m.PrivacySettings })),
);
const RegulatorySettings = lazy(() =>
  withTexts(import("@/features/settings/RegulatorySettings")).then((m) => ({
    default: m.RegulatorySettings,
  })),
);
const StaffScreen = lazy(() =>
  withTexts(import("@/features/staff/StaffScreen")).then((m) => ({ default: m.StaffScreen })),
);
const ZakatScreen = lazy(() =>
  withTexts(import("@/features/zakat/ZakatScreen")).then((m) => ({ default: m.ZakatScreen })),
);
const StockScreen = lazy(() =>
  withTexts(import("@/features/stock/StockScreen")).then((m) => ({ default: m.StockScreen })),
);

export function AppRoutes({ session }: { session: Session }) {
  const firstSection = allowedSections(session)[0]?.path ?? "about";
  // The UI hides what a role cannot do; Rust re-checks every command anyway (spec §4.5).
  const guard = (permission: Permission, element: ReactElement) =>
    session.permissions.includes(permission) ? element : <Navigate to="/settings/about" replace />;
  // Modules: any of the listed permissions opens the screen (Rust re-checks every command).
  const home = backOfficeHome(session);
  const allow = (anyOf: readonly Permission[], element: ReactElement) =>
    anyOf.some((p) => session.permissions.includes(p)) ? element : <Navigate to={home} replace />;
  // A pack screen (M8): its rail entry's permissions, then its module (`PackGate`).
  const pack = (path: string, element: ReactElement) => {
    const item = PACK_ITEMS.find((candidate) => candidate.path === `/${path}`);
    return item?.modules ? (
      allow(item.anyOf, <PackGate module={item.modules}>{element}</PackGate>)
    ) : (
      <Navigate to={home} replace />
    );
  };
  const routes = (
    <Routes>
      <Route element={<SessionFrame session={session} />}>
        {/* the landing redirect renders no shell: a cashier goes straight to the till */}
        <Route index element={<Navigate to={landingRoute(session)} replace />} />
        {/* UX-020: the till the shop's mode calls for (the floor of a restaurant); every till screen
            sells only with a licence that allows writes */}
        <Route
          path="pos"
          element={allow(
            ["sales.create"],
            <PosLicenseGate session={session}>
              <TillEntry session={session} />
            </PosLicenseGate>,
          )}
        />
        <Route
          path="pos/sale"
          element={allow(
            ["sales.create"],
            <PosLicenseGate session={session}>
              <TillEntry session={session} direct />
            </PosLicenseGate>,
          )}
        />
        <Route
          path="pos/floor"
          element={allow(
            ["sales.create"],
            <PosLicenseGate session={session}>
              <PackGate module="restaurant">
                <FloorScreen session={session} />
              </PackGate>
            </PosLicenseGate>,
          )}
        />
        <Route
          path="pos/ticket/:id"
          element={allow(
            ["sales.create"],
            <PosLicenseGate session={session}>
              <PackGate module="restaurant">
                <TicketRoute session={session} />
              </PackGate>
            </PosLicenseGate>,
          )}
        />
        <Route element={<AppShell session={session} />}>
          {MODULE_ITEMS.map((item) => (
            <Route
              key={item.path}
              path={item.path.slice(1)}
              element={<ComingSoon labelKey={item.labelKey} />}
            />
          ))}
          <Route path="dashboard" element={allow(["reports.sales"], <DashboardPage />)} />
          {/* each report kind checks its own permission; the catalogue offers the ones the role may run */}
          <Route path="reports" element={allow(REPORT_PERMISSIONS, <ReportsPage />)} />
          {/* static before `:kind` (react-router ranks it first anyway): the G50, registers and archive */}
          <Route path="reports/fiscal" element={allow(["reports.tax"], <FiscalPage session={session} />)} />
          <Route path="reports/:kind" element={allow(REPORT_PERMISSIONS, <ReportView />)} />
          <Route
            path="cash"
            element={allow(
              ["cash.open", "cash.close", "cash.view_history"],
              <CashScreen session={session} />,
            )}
          />
          {/* the DZBuild store's orders; the rail shows them while a store is linked */}
          <Route path="orders" element={allow(["sales.create"], <OrdersScreen session={session} />)} />
          <Route
            path="sales"
            element={allow(
              ["sales.view_all", "sales.reprint", "sales.return"],
              <SalesHistory session={session} />,
            )}
          />
          {/* the archive then lists the kinds each permission may see */}
          <Route path="documents" element={allow(ARCHIVE_PERMISSIONS, <DocumentsPage session={session} />)} />
          {/* the kinds the user may make; a draft opens in its editor from its own address */}
          <Route
            path="documents/new/:kind"
            element={allow(DOCUMENT_PERMISSIONS, <NewDocumentRoute session={session} />)}
          />
          <Route
            path="documents/:id"
            element={allow(ARCHIVE_PERMISSIONS, <DocumentRoute session={session} />)}
          />
          <Route path="products" element={allow(["products.view"], <ProductsScreen session={session} />)} />
          <Route path="products/promotions" element={allow(["products.view"], <PromotionsPage />)} />
          {/* the templates and the reprint queue are `products.labels`, the scale items `products.view` */}
          <Route
            path="products/labels"
            element={allow(["products.labels", "products.view"], <LabelsPage />)}
          />
          {/* a counter holding only `stock.count` reaches the counts */}
          <Route path="stock" element={allow(["stock.view", "stock.count"], <StockScreen />)} />
          <Route path="customers" element={allow(["customers.view"], <CustomersPage />)} />
          <Route path="customers/:id" element={allow(["customers.view"], <CustomerDetail />)} />
          {/* the directory is also the supplier keeper's; the orders and documents are purchases */}
          <Route
            path="purchases"
            element={allow(["purchases.view", "suppliers.manage"], <PurchasesPage />)}
          />
          <Route path="purchases/orders/:id" element={allow(["purchases.view"], <OrderEditor />)} />
          <Route
            path="purchases/suppliers/:id"
            element={allow(["purchases.view", "suppliers.manage"], <SupplierDetail />)}
          />
          {/* each import checks its own permission; the page offers the ones the role may run */}
          <Route
            path="imports/:entity"
            element={allow(
              ["products.import", "customers.manage", "suppliers.manage", "stock.count"],
              <ImportPage session={session} />,
            )}
          />
          {/* M8 vertical packs: each screen behind its rail entry's permissions and its module
              (`pack`: PACK_ITEMS, then PackGate). `/customer-orders` (`/orders` is the DZBuild
              store's); the jewellery screen admits `products.view`, `sales.create` or
              `customers.manage` (its register tab needs `customers.manage`); the staff files and
              advances are `settings.users`'s, the shifts `reports.cash`'s. */}
          <Route path="restaurant/*" element={pack("restaurant", <RestaurantAdmin session={session} />)} />
          <Route path="services/*" element={pack("services", <ServicesScreen />)} />
          <Route path="instalments/*" element={pack("instalments", <InstalmentsScreen />)} />
          <Route path="rounds/*" element={pack("rounds", <RoundsScreen />)} />
          <Route path="customer-orders/*" element={pack("customer-orders", <CustomerOrdersScreen />)} />
          <Route path="loyalty/*" element={pack("loyalty", <LoyaltyScreen session={session} />)} />
          <Route path="jewellery/*" element={pack("jewellery", <JewelleryScreen session={session} />)} />
          <Route path="staff/*" element={pack("staff", <StaffScreen session={session} />)} />
          <Route path="zakat/*" element={pack("zakat", <ZakatScreen />)} />
          {/* every signed-in user: F1 opens the page of the screen (help:<topic>) */}
          <Route path="help" element={<HelpCentre />} />
          <Route path="help/:topic" element={<HelpRoute />} />
          <Route path="settings" element={<SettingsLayout session={session} />}>
            <Route index element={<Navigate to={firstSection} replace />} />
            <Route path="shop" element={guard("settings.company", <ShopSettings />)} />
            <Route path="activity" element={guard("settings.company", <ActivitySettings />)} />
            <Route path="documents" element={guard("settings.company", <DocumentsSettings />)} />
            <Route path="sales" element={guard("settings.company", <SalesRulesSettings />)} />
            <Route path="payments" element={guard("settings.company", <PaymentsSettings />)} />
            <Route path="keys" element={guard("settings.company", <KeysSettings />)} />
            <Route
              path="expense-categories"
              element={guard("settings.company", <ExpenseCategoriesSettings />)}
            />
            <Route path="users" element={guard("settings.users", <UsersSettings session={session} />)} />
            <Route path="privacy" element={guard("settings.company", <PrivacySettings />)} />
            <Route path="hardware" element={guard("settings.hardware", <HardwareHub />)} />
            <Route path="network" element={guard("settings.data", <NetworkSettings session={session} />)} />
            <Route path="display" element={guard("settings.hardware", <DisplaySettings />)} />
            <Route path="backups" element={guard("settings.data", <BackupsSettings session={session} />)} />
            <Route
              path="dzbuild"
              element={guard("settings.dzbuild", <DzbuildSettings session={session} />)}
            />
            <Route path="updates" element={guard("settings.data", <UpdatesSettings session={session} />)} />
            <Route
              path="diagnostics"
              element={guard("settings.data", <DiagnosticsSettings session={session} />)}
            />
            <Route path="data" element={guard("settings.data", <DataSettings />)} />
            <Route path="regulatory" element={guard("settings.company", <RegulatorySettings />)} />
            <Route path="audit" element={guard("audit.view", <AuditSettings />)} />
            <Route path="about" element={<AboutSettings session={session} />} />
            <Route path="*" element={<Navigate to="/settings/about" replace />} />
          </Route>
          <Route path="*" element={<Navigate to={home} replace />} />
        </Route>
      </Route>
    </Routes>
  );
  // the till's parts its pack screens use (`features/pos/till-kit.ts`)
  return <TillKitContext.Provider value={tillKit}>{routes}</TillKitContext.Provider>;
}
