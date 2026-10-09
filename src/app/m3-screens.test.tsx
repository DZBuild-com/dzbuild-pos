import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { setLanguage } from "@/lib/i18n";
import { mockCommands, renderWithProviders } from "@/test/render";
import { ownerSession } from "@/test/fixtures";
import { archivePageFixture, mentionsCompleteFixture } from "@/test/fixtures-m3";
import { activeStatus } from "@/test/fixtures-m9";
import { AppRoutes } from "./routes";

const handlers = {
  documents_search: () => archivePageFixture,
  // M9: the shell's licence banner (a lifetime licence: no banner)
  license_status: () => activeStatus,
  documents_mentions: () => mentionsCompleteFixture,
  fiscal_g50: () => ({
    from: "2026-09-01",
    to: "2026-09-30",
    form: "G n° 50",
    lines: [],
    otherRates: [],
    tvaCollected: 0,
    tvaDeductible: 0,
    tvaDue: 0,
    tvaCredit: 0,
    stampDuty: 0,
  }),
  settings_get: () => null,
  hardware_get: () => ({
    version: 1,
    printers: [],
    receiptPrinterId: null,
    receipt: {},
    scanner: { layout: "us" },
    documents: { defaultPrinter: null, kinds: [], marginsMm: 12 },
  }),
  hardware_discover: () => [],
  categories_list: () => [],
};

describe("the M3 screens open in the three languages", () => {
  it.each([
    ["ar", "rtl", "الوثائق"],
    ["fr", "ltr", "Documents"],
    ["en", "ltr", "Documents"],
  ] as const)("%s", async (lang, dir, title) => {
    // The signed-in user's language is the one the frame applies (`SessionFrame`).
    const session = { ...ownerSession, lang };
    await setLanguage(lang);
    mockCommands(handlers);
    const { unmount } = renderWithProviders(<AppRoutes session={session} />, { route: "/documents" });
    expect(await screen.findByRole("heading", { name: title })).toBeInTheDocument();
    expect(document.documentElement.dir).toBe(dir);
    expect(document.documentElement.lang).toBe(lang);
    unmount();
    for (const route of ["/documents/new/invoice", "/settings/documents", "/reports/fiscal"]) {
      const view = renderWithProviders(<AppRoutes session={session} />, { route });
      expect(await view.findAllByRole("heading")).not.toHaveLength(0);
      view.unmount();
    }
  });
});
