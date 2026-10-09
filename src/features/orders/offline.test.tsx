import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AppRoutes } from "@/app/routes";
import { i18n } from "@/lib/i18n";
import { ownerSession, ownerTile, updateStatusFixture } from "@/test/fixtures";
import { dashboardFixture } from "@/test/fixtures-m2";
import { lanStandaloneFixture } from "@/test/fixtures-m5";
import { unlinkedStatus } from "@/test/fixtures-m7";
import { packState } from "@/test/fixtures-m8";
import { activeStatus } from "@/test/fixtures-m9";
import { mockCommands, renderWithProviders } from "@/test/render";

const offline = () => Promise.reject({ code: "cloud_unavailable" });

/** The link is optional: every screen renders unlinked, and linked with DZBuild unreachable. */
describe("every_screen_works_unlinked_and_offline", () => {
  const common = {
    cloud_status: () => unlinkedStatus,
    cloud_notices: () => [],
    cloud_outbox_failed: () => [],
    cloud_publish_list: () => [],
    cloud_devices: offline,
    cloud_orders_list: () => [],
    settings_get: () => false,
    backup_list: () => [],
    backup_status: () => ({ lastAuto: null, newestAt: null, stale: false, databaseDamaged: false }),
    cash_current: () => null,
    hardware_get: () => ({ receiptPrinterId: null }),
    company_get: () => null,
    auth_session: () => ownerSession,
    // what the M0-M2 screens ask at mount (their own tests answer the same)
    cash_sessions: () => [],
    auth_list_users: () => [ownerTile],
    reports_dashboard: () => dashboardFixture,
    reports_catalog: () => [],
    // M5: the shell's network pill and banner (a single PC here)
    lan_status: () => lanStandaloneFixture,
    // M6: the shell's demo ribbon and update pill, and the Backups screen's second copy and sticks
    demo_status: () => ({ active: false, exists: false, ownerPin: null, cashierPin: null }),
    updates_status: () => updateStatusFixture,
    backup_mirror_status: () => ({
      folder: null,
      label: null,
      available: false,
      freeBytes: null,
      lastCopyAt: null,
      lastError: null,
    }),
    backup_drives: () => [],
    // M8: the cash page asks which pack modules are on (its Flexy tab)
    packs_state: () => packState(),
    // M9: the shell's licence banner (a lifetime licence: no banner)
    license_status: () => activeStatus,
  };

  /** Each route and the heading its screen shows (not an error page). */
  const screens: [route: string, heading: string, level: number][] = [
    ["/settings/dzbuild", "sync:section", 2],
    ["/settings/activity", "packs:settings.activityTitle", 2],
    // M6: the Backups screen ends its cards with the export of the `.dzpos` file
    ["/settings/backups", "settings:backups.export.title", 2],
    ["/orders", "nav:orders", 1],
    ["/cash", "cash:title", 1],
    ["/dashboard", "nav:dashboard", 1],
  ];

  for (const [route, heading, level] of screens) {
    it(`renders ${route} unlinked`, async () => {
      const calls = mockCommands(common);
      renderWithProviders(<AppRoutes session={ownerSession} />, { route });
      // the main menu: the settings screens have a navigation of their own
      expect(await screen.findByLabelText(i18n.t("nav:mainMenu"), { selector: "nav" })).toBeInTheDocument();
      expect(await screen.findByText(i18n.t(heading), { selector: `h${level}` })).toBeInTheDocument();
      await waitFor(() => expect(screen.queryByText(i18n.t("common:status.loading"))).toBeNull());
      // a command nobody answers would fail the screen like an unknown backend error
      expect([...new Set(calls.filter((c) => !(c in common) && !c.startsWith("plugin:")))]).toEqual([]);
      expect(screen.queryByRole("alert", { name: /cloud_/ })).not.toBeInTheDocument();
    });
  }

  it("linked and offline, Settings › DZBuild says so without an error page", async () => {
    mockCommands({
      ...common,
      cloud_status: () => ({
        ...unlinkedStatus,
        linked: true,
        linkStatus: "linked",
        storeName: "Boutique Amel",
      }),
    });
    renderWithProviders(<AppRoutes session={ownerSession} />, { route: "/settings/dzbuild" });
    expect(await screen.findByText(/Hors ligne : tout est gardé/)).toBeInTheDocument();
  });
});
