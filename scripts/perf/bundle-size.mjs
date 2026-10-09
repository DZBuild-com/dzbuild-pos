// NFR-010: the JavaScript the /pos route needs stays under 300 kB gzipped in each of the three
// languages (the entry plus that language's till texts, `src/locales/bundles/<lang>-till.ts`, read
// before the first paint), the back office is code-split, and the E2E fake backend never ships.
// Fonts ship as WOFF2 only (NFR-002): WebView2 never reads fontsource's WOFF fallbacks, which
// would only grow the online setup (`vite.config.ts`, `woff2Only`).
// Run after `pnpm build`: `pnpm perf:bundle`.
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { gzipSync } from "node:zlib";

export const BUDGET_BYTES = 300 * 1024;
/** The till's pack extensions and pack screens load as chunks of their own, on demand;
 * together they stay under 120 kB gzipped (their own JavaScript, not what the /pos entry already
 * holds). */
export const PACK_BUDGET_BYTES = 120 * 1024;
export const E2E_MARKER = "__DZPOS_E2E_FAKE_BACKEND__";
/** A language's till texts (finding 28): one of them loads with the /pos route. */
export const TILL_TEXTS = /^src\/locales\/bundles\/(\w+)-till\.ts$/;
/** Every routed screen but the sale screen (`src/app/routes.tsx` loads each with `lazy`). */
export const LAZY_SCREENS = [
  "src/features/cash/CashScreen.tsx",
  "src/features/customers/CustomerDetail.tsx",
  "src/features/customers/CustomersPage.tsx",
  // M8: the customer orders and appointments, an order put in or served at the till
  "src/features/customer-orders/CustomerOrdersScreen.tsx",
  "src/features/customer-orders/OrderDialog.tsx",
  "src/features/customer-orders/OrderFulfil.tsx",
  "src/features/dashboard/DashboardPage.tsx",
  "src/features/documents/DocumentView.tsx",
  "src/features/documents/DocumentsPage.tsx",
  "src/features/documents/SalesHistory.tsx",
  "src/features/documents/editor/DocumentEditor.tsx",
  "src/features/fiscal/FiscalPage.tsx",
  "src/features/help/HelpCentre.tsx",
  "src/features/imports/ImportWizard.tsx",
  // M8: the instalment contracts, and a contract signed from the till
  "src/features/instalments/InstalmentDialog.tsx",
  "src/features/instalments/InstalmentsScreen.tsx",
  "src/features/jewellery/JewelleryScreen.tsx",
  "src/features/labels/LabelsPage.tsx",
  "src/features/loyalty/LoyaltyScreen.tsx",
  // M8: the gate of every pack screen, then the pack screens and the till's pack
  // extensions (Task 24)
  "src/features/packs/PackGate.tsx",
  "src/features/pos/till-packs.tsx",
  "src/features/restaurant/FloorScreen.tsx",
  "src/features/restaurant/RestaurantAdmin.tsx",
  "src/features/restaurant/TicketScreen.tsx",
  // the pharmacy, butcher and catalogue pages of the products and stock screens (Task 25)
  "src/features/pharmacy/MedicineTab.tsx",
  "src/features/pharmacy/ExpiryBoard.tsx",
  "src/features/pharmacy/ChifaImport.tsx",
  "src/features/pharmacy/ControlledRegister.tsx",
  "src/features/butcher/YieldScreen.tsx",
  "src/features/butcher/WasteScreen.tsx",
  "src/features/products/VariantGrid.tsx",
  "src/features/products/RegulatedBoard.tsx",
  "src/features/products/DepartmentKeysEditor.tsx",
  "src/features/products/ProductsScreen.tsx",
  "src/features/promotions/PromotionsPage.tsx",
  "src/features/purchases/OrderEditor.tsx",
  "src/features/purchases/PurchasesPage.tsx",
  "src/features/purchases/SupplierDetail.tsx",
  "src/features/reports/ReportView.tsx",
  "src/features/reports/ReportsPage.tsx",
  // M8: the repair board, and a repair handed back at the till (a pack chunk)
  "src/features/services/ServicePickup.tsx",
  "src/features/services/ServicesScreen.tsx",
  "src/features/settings/AboutSettings.tsx",
  "src/features/settings/ActivitySettings.tsx",
  "src/features/settings/AuditSettings.tsx",
  "src/features/settings/BackupsSettings.tsx",
  "src/features/settings/DataSettings.tsx",
  "src/features/settings/DiagnosticsSettings.tsx",
  "src/features/settings/DisplaySettings.tsx",
  "src/features/settings/DocumentsSettings.tsx",
  "src/features/settings/ExpenseCategoriesSettings.tsx",
  "src/features/settings/KeysSettings.tsx",
  "src/features/settings/PaymentsSettings.tsx",
  "src/features/settings/PrivacySettings.tsx",
  "src/features/settings/RegulatorySettings.tsx",
  "src/features/settings/SalesRulesSettings.tsx",
  "src/features/settings/SettingsLayout.tsx",
  "src/features/settings/ShopSettings.tsx",
  "src/features/settings/UpdatesSettings.tsx",
  "src/features/settings/UsersSettings.tsx",
  "src/features/settings/hardware/HardwareHub.tsx",
  "src/features/staff/StaffScreen.tsx",
  "src/features/stock/StockScreen.tsx",
  // M8: the delivery rounds
  "src/features/wholesale/RoundsScreen.tsx",
  // M8: a gift voucher sold from the till's menu (a pack chunk)
  "src/features/loyalty/SellVoucherDialog.tsx",
  // The activation screen and the read-only banner and till card (their words load with them,
  // only on a PC that needs them). Settings › Licence is not in the free edition's bundle at all.
  "src/features/license/ActivationScreen.tsx",
  "src/features/license/LicenseNotice.tsx",
  "src/features/license/PosLicenseCard.tsx",
  "src/features/zakat/ZakatScreen.tsx",
];

/** The till's pack extensions and pack screens: what the till opens on demand while a
 * pack module is on (the pack dialogs, the restaurant's floor and table ticket, the
 * sale screen's pack chunk). Together within `PACK_BUDGET_BYTES` beyond what /pos already loaded. */
export const PACK_SCREENS = [
  "src/features/services/ServicePickup.tsx",
  "src/features/instalments/InstalmentDialog.tsx",
  "src/features/customer-orders/OrderDialog.tsx",
  "src/features/customer-orders/OrderFulfil.tsx",
  "src/features/loyalty/SellVoucherDialog.tsx",
  "src/features/restaurant/FloorScreen.tsx",
  "src/features/restaurant/TicketScreen.tsx",
  "src/features/pos/till-packs.tsx",
];

export function checkBundle(distDir, { lazyScreens = LAZY_SCREENS, packScreens = PACK_SCREENS } = {}) {
  const manifest = JSON.parse(readFileSync(path.join(distDir, ".vite", "manifest.json"), "utf8"));
  const problems = [];
  // The app's page; the build has others (the print host, the customer screen).
  const entryKey = manifest["index.html"]?.isEntry
    ? "index.html"
    : Object.keys(manifest).find((k) => manifest[k].isEntry);
  const seen = new Set();
  const walk = (key) => {
    if (seen.has(key) || !manifest[key]) return;
    seen.add(key);
    for (const next of manifest[key].imports ?? []) walk(next);
  };
  if (entryKey) walk(entryKey);
  const gzipped = (key) => gzipSync(readFileSync(path.join(distDir, manifest[key].file))).length;
  let entryGzipBytes = 0;
  for (const key of seen) entryGzipBytes += gzipped(key);
  // Each language's till texts, with what they import that the entry does not hold already.
  const tillTexts = {};
  for (const [key, chunk] of Object.entries(manifest)) {
    const lang = TILL_TEXTS.exec(chunk.src ?? key)?.[1];
    if (!lang) continue;
    const own = new Set();
    const collect = (k) => {
      if (own.has(k) || seen.has(k) || !manifest[k]) return;
      own.add(k);
      for (const next of manifest[k].imports ?? []) collect(next);
    };
    collect(key);
    tillTexts[lang] = [...own].reduce((sum, k) => sum + gzipped(k), 0);
  }
  const largest = Object.entries(tillTexts).sort((a, b) => b[1] - a[1])[0];
  const posGzipBytes = entryGzipBytes + (largest?.[1] ?? 0);
  if (posGzipBytes > BUDGET_BYTES) {
    const which = largest ? ` in ${largest[0]}` : "";
    problems.push(`the /pos route loads ${(posGzipBytes / 1024).toFixed(1)} kB gzipped${which}, over 300 kB`);
  }
  // The pack chunks: each pack screen with what it imports that the /pos entry does not hold, on
  // its own (for the report) and all together (the budget: a chunk two screens share counts once).
  const pack = new Set();
  const packs = {};
  for (const screen of packScreens) {
    const key = Object.keys(manifest).find((k) => (manifest[k].src ?? k) === screen);
    if (!key) continue;
    const own = new Set();
    const collect = (k) => {
      if (own.has(k) || seen.has(k) || !manifest[k]) return;
      own.add(k);
      for (const next of manifest[k].imports ?? []) collect(next);
    };
    collect(key);
    packs[screen] = [...own].reduce((sum, k) => sum + gzipped(k), 0);
    for (const k of own) pack.add(k);
  }
  const packGzipBytes = [...pack].reduce((sum, k) => sum + gzipped(k), 0);
  if (packGzipBytes > PACK_BUDGET_BYTES) {
    problems.push(`the till's pack chunks load ${(packGzipBytes / 1024).toFixed(1)} kB gzipped, over 120 kB`);
  }
  for (const screen of lazyScreens) {
    const chunk = Object.values(manifest).find((c) => c.src === screen);
    if (!chunk?.isDynamicEntry) problems.push(`${screen} is not lazy (it would load with the till)`);
  }
  for (const chunk of Object.values(manifest)) {
    const file = path.join(distDir, chunk.file);
    if (existsSync(file) && readFileSync(file, "utf8").includes(E2E_MARKER)) {
      problems.push(`${chunk.file} contains the E2E fake backend`);
    }
  }
  const assets = path.join(distDir, "assets");
  for (const name of existsSync(assets) ? readdirSync(assets) : []) {
    if (name.endsWith(".woff")) problems.push(`assets/${name}: a WOFF font (WebView2 reads the WOFF2 one)`);
  }
  return { posGzipBytes, entryGzipBytes, tillTexts, packGzipBytes, packScreens: packs, problems };
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const report = checkBundle(path.resolve("dist"));
  const kb = (bytes) => `${(bytes / 1024).toFixed(1)} kB`;
  const langs = Object.entries(report.tillTexts)
    .map(([lang, bytes]) => `${lang} ${kb(report.entryGzipBytes + bytes)}`)
    .join(", ");
  console.log(`/pos route: ${kb(report.posGzipBytes)} gzipped (budget 300 kB; ${langs || "no till texts"})`);
  console.log(`/pos pack chunks: ${kb(report.packGzipBytes)} gzipped (budget 120 kB)`);
  const packs = Object.entries(report.packScreens)
    .map(([screen, bytes]) => `${path.basename(screen, ".tsx")} ${kb(bytes)}`)
    .join(", ");
  console.log(`  each beyond /pos: ${packs || "none"}`);
  if (report.problems.length) {
    console.error(report.problems.join("\n"));
    process.exit(1);
  }
}
