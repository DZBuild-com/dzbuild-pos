import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WasteScreen } from "@/features/butcher/WasteScreen";
import { YieldScreen } from "@/features/butcher/YieldScreen";
import { CustomerOrdersScreen } from "@/features/customer-orders/CustomerOrdersScreen";
import { InstalmentsScreen } from "@/features/instalments/InstalmentsScreen";
import { JewelleryScreen } from "@/features/jewellery/JewelleryScreen";
import { ActivationScreen } from "@/features/license/ActivationScreen";
import { LicenseSettings } from "@/features/license/LicenseSettings";
import { LoyaltyScreen } from "@/features/loyalty/LoyaltyScreen";
import { ChifaImport } from "@/features/pharmacy/ChifaImport";
import { ControlledRegister } from "@/features/pharmacy/ControlledRegister";
import { ExpiryBoard } from "@/features/pharmacy/ExpiryBoard";
import { FlexyScreen } from "@/features/phones/FlexyScreen";
import { DepartmentKeysEditor } from "@/features/products/DepartmentKeysEditor";
import { RegulatedBoard } from "@/features/products/RegulatedBoard";
import { FloorScreen } from "@/features/restaurant/FloorScreen";
import { RestaurantAdmin } from "@/features/restaurant/RestaurantAdmin";
import { TicketScreen } from "@/features/restaurant/TicketScreen";
import { ServicesScreen } from "@/features/services/ServicesScreen";
import { rememberSetupLang } from "@/features/setup/setup-lang";
import { ActivitySettings } from "@/features/settings/ActivitySettings";
import { KeysSettings } from "@/features/settings/KeysSettings";
import { PaymentsSettings } from "@/features/settings/PaymentsSettings";
import { PrivacySettings } from "@/features/settings/PrivacySettings";
import { RegulatorySettings } from "@/features/settings/RegulatorySettings";
import { StaffScreen } from "@/features/staff/StaffScreen";
import { RoundsScreen } from "@/features/wholesale/RoundsScreen";
import { ZakatScreen } from "@/features/zakat/ZakatScreen";
import { i18n } from "@/lib/i18n";
import { chifaFile, mockEveryPack, sessionWith } from "@/test/fixtures-m8";
import { unlicensedStatus } from "@/test/fixtures-m9";
import { missingKeys, mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";

// UX-001 for the M8 screens (and M9's licence screens): each one in Arabic, French and English,
// every tab opened, without a key missing in the language (not found at all, or found only in
// the French fallback), without a raw identifier on screen, right to left in Arabic only.
const everyone = sessionWith("all");
const SCREENS: Record<string, ReactElement> = {
  // Task 23: the restaurant
  FloorScreen: <FloorScreen session={everyone} />,
  TicketScreen: <TicketScreen session={everyone} ticketId="t1" />,
  RestaurantAdmin: <RestaurantAdmin session={everyone} />,
  // Task 25: the pharmacy, the butcher and the catalogue's modules
  ExpiryBoard: <ExpiryBoard />,
  ChifaImport: <ChifaImport initialFile={chifaFile()} />,
  ControlledRegister: <ControlledRegister />,
  YieldScreen: <YieldScreen />,
  WasteScreen: <WasteScreen />,
  RegulatedBoard: <RegulatedBoard />,
  DepartmentKeysEditor: <DepartmentKeysEditor />,
  // Tasks 26–28: services and flexy, instalments, customer orders, rounds
  ServicesScreen: <ServicesScreen />,
  FlexyScreen: <FlexyScreen />,
  InstalmentsScreen: <InstalmentsScreen />,
  CustomerOrdersScreen: <CustomerOrdersScreen />,
  RoundsScreen: <RoundsScreen />,
  // Tasks 29–30: loyalty, jewellery, staff, zakat and the settings
  LoyaltyScreen: <LoyaltyScreen session={everyone} />,
  JewelleryScreen: <JewelleryScreen session={everyone} />,
  StaffScreen: <StaffScreen session={everyone} />,
  ZakatScreen: <ZakatScreen />,
  ActivitySettings: <ActivitySettings />,
  KeysSettings: <KeysSettings />,
  PaymentsSettings: <PaymentsSettings />,
  PrivacySettings: <PrivacySettings />,
  RegulatorySettings: <RegulatorySettings />,
  // M9: the activation screen of a new install and Settings › Licence
  ActivationScreen: <ActivationScreen status={unlicensedStatus} />,
  LicenseSettings: <LicenseSettings session={everyone} />,
};

/** A value shown as the code holds it (`in_progress`, `food_service`) instead of its words. */
const RAW_IDENTIFIER = /^[a-z][a-z0-9]*(?:_[a-z0-9]+)+$/;

/** The raw identifiers among the words of the page's texts and accessible names. */
function rawIdentifiers(): string[] {
  const texts: string[] = [];
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
    texts.push(node.nodeValue ?? "");
  }
  for (const element of document.body.querySelectorAll("[aria-label], [title]")) {
    texts.push(element.getAttribute("aria-label") ?? "", element.getAttribute("title") ?? "");
  }
  return texts.flatMap((text) => text.split(/[\s,;:()«»"'.!?]+/)).filter((word) => RAW_IDENTIFIER.test(word));
}

/** Every tab of the screen opened in turn, each waiting for its panel to finish loading. */
async function openEveryTab(): Promise<void> {
  const loading = i18n.t("common:status.loading");
  await waitFor(() => expect(screen.queryByText(loading)).toBeNull());
  const count = screen.queryAllByRole("tab").length;
  for (let index = 0; index < count; index += 1) {
    const tab = screen.getAllByRole("tab")[index];
    if (!tab || tab.getAttribute("aria-disabled") === "true") continue;
    await userEvent.click(tab);
    await waitFor(() => expect(screen.queryByText(loading)).toBeNull());
  }
}

beforeEach(() => {
  // jsdom has no layout: M1b's product grid (TanStack Virtual) needs its scroller's size
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1000);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(700);
});

afterEach(() => {
  cleanup();
  clearMocks();
});

describe.each(["ar", "fr", "en"] as const)("pack screens in %s", (lang) => {
  it.each(Object.entries(SCREENS))("%s renders without a missing key", async (_name, ui) => {
    const missing: string[] = [];
    const onMissing = (_lngs: readonly string[], ns: string, key: string) => missing.push(`${ns}:${key}`);
    i18n.on("missingKey", onMissing);
    missingKeys.clear();
    const backend = mockEveryPack(lang);
    const calls = mockCommands(backend);
    // this PC's last choice: the activation screen opens in it
    rememberSetupLang(lang);
    await renderM8(ui, { lang, session: sessionWith("all", lang) });
    expect(await screen.findAllByRole("heading")).not.toHaveLength(0);
    await openEveryTab();
    expect(document.documentElement.dir).toBe(lang === "ar" ? "rtl" : "ltr");
    i18n.off("missingKey", onMissing);
    expect({
      missing: [...new Set([...missing, ...missingKeys])],
      raw: rawIdentifiers(),
      // every command the screen called was answered: its content is on screen, not an error
      unanswered: [...new Set(calls.filter((cmd) => !(cmd in backend) && !cmd.startsWith("plugin:")))],
    }).toEqual({ missing: [], raw: [], unanswered: [] });
  });
});
