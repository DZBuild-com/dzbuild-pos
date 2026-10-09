import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { emit } from "@tauri-apps/api/event";
import { describe, expect, it, vi } from "vitest";
import { CustomerDisplay } from "@/customer-display/CustomerDisplay";
import { LabelsPage } from "@/features/labels/LabelsPage";
import { HardwareHub } from "@/features/settings/hardware/HardwareHub";
import { i18n, type Lang, setLanguage } from "@/lib/i18n";
import type { DisplayFrame, HardwareConfig, Session } from "@/lib/ipc/types";
import { ownerSession, settingsValues } from "@/test/fixtures";
import { categoriesFixture } from "@/test/fixtures-m1";
import {
  frameFixture,
  hardwareV2Fixture,
  labelBatchFixture,
  labelTemplatesFixture,
  scaleItemsFixture,
  staleLabelsFixture,
  weightFixture,
} from "@/test/fixtures-m4";
import { lanStandaloneFixture } from "@/test/fixtures-m5";
import { packState } from "@/test/fixtures-m8";
import { type Handler, missingKeys, mockCommands, renderWithProviders } from "@/test/render";

const serial = (port: string) =>
  ({ port, baud: 9600, dataBits: "eight", parity: "none", stopBits: "one" }) as const;

/** A butcher-greengrocer's PC with every M4 device configured: a label printer, a Toledo scale,
 * a pole display and the customer screen, a serial scanner and a drawer adapter on COM. */
const equipped: HardwareConfig = {
  ...hardwareV2Fixture,
  labelPrinters: [
    {
      id: "l1",
      name: "Étiquettes",
      transport: { kind: "windows", printerName: "XP-365B" },
      profile: {
        preset: "xprinter",
        language: "tspl",
        dpi: "d203",
        darkness: 8,
        speed: 4,
        media: "gap",
        offsetXDmm: 0,
        offsetYDmm: 0,
        bitmapBlackIsZero: true,
      },
    },
  ],
  labelPrinterId: "l1",
  scale: {
    serial: { ...serial("COM3"), dataBits: "seven", parity: "even" },
    protocol: "toledo8217",
    pollMs: 200,
    stableReadings: 3,
    capacityG: 15_000,
  },
  display: {
    vfd: {
      serial: serial("COM2"),
      commands: "esc_pos",
      columns: 20,
      rows: 2,
      latinLang: "fr",
      charset: "pc858",
      codePageSelector: 19,
    },
    screen: { enabled: true, monitor: "\\\\.\\DISPLAY2", showLogo: true, thanksSeconds: 5 },
  },
  scanner: {
    layout: "azerty",
    prefix: "]E0",
    suffix: "enter_or_tab",
    serial: { serial: serial("COM5"), prefix: "", suffix: "cr_lf" },
  },
  drawer: { kind: "serial", port: "COM4", baud: 9600, kick: "dtr" },
};

const png = "data:image/png;base64,iVBORw0KGgo=";

/** The commands the M4 screens call, answered with the M4 fixtures. Returns the calls no fixture
 * answered (such a tab would show an error, not its content). */
function mockM4(session: Session): () => string[] {
  const handlers: Record<string, Handler> = {
    auth_session: () => session,
    settings_get: ({ key }) => ({ ...settingsValues, "scale.barcode_formats": null })[String(key)],
    datapack_get: () => ({
      scaleBarcodes: {
        source: "",
        defaultLayouts: [
          { prefix: "21", pluDigits: 5, verifier: false, value: "weight", valueDigits: 5, decimals: 3 },
          { prefix: "22", pluDigits: 5, verifier: false, value: "price", valueDigits: 5, decimals: 2 },
        ],
      },
    }),
    hardware_get: () => equipped,
    hardware_discover: () => [
      {
        kind: "windows_printer",
        name: "XP-365B",
        port: "USB002",
        driver: "XP-365B",
        isDefault: false,
        offline: false,
        likelyReceipt: false,
        likelyLabel: true,
      },
      { kind: "serial_port", port: "COM3", description: "USB-SERIAL CH340" },
    ],
    hardware_printer_status: () => ({
      reachable: true,
      paperOut: false,
      paperLow: false,
      coverOpen: false,
      error: false,
      source: "windows",
    }),
    devices_status: () => [
      { device: "scale", state: { state: "connected", port: "COM3" } },
      { device: "vfd", state: { state: "disconnected", port: "COM2", code: "port_unavailable" } },
      { device: "serial_scanner", state: { state: "connected", port: "COM5" } },
    ],
    scale_latest: () => weightFixture,
    display_monitors: () => [
      { name: "\\\\.\\DISPLAY1", width: 1366, height: 768, x: 0, y: 0, primary: true, mainWindow: true },
      { name: "\\\\.\\DISPLAY2", width: 1024, height: 768, x: 1366, y: 0, primary: false, mainWindow: false },
    ],
    labels_templates: () => labelTemplatesFixture,
    labels_preview: () => ({ png, label: labelBatchFixture.groups[0]!.labels[0], dpi: "d203", warnings: [] }),
    labels_stale: () => staleLabelsFixture,
    catalog_categories_list: () => categoriesFixture,
    scale_items_list: () => scaleItemsFixture,
    // M5: the scale items ask whether this PC is a till (a single PC here)
    lan_status: () => lanStandaloneFixture,
    // M8: the label designer and the hardware hub ask which pack modules are on (none here)
    packs_state: () => packState(),
  };
  const calls = mockCommands(handlers);
  return () => [...new Set(calls.filter((c) => !(c in handlers) && !c.startsWith("plugin:")))];
}

/** Clicks every tab of the screen, waiting each time for its panel to finish loading. */
async function clickEveryTab(): Promise<string[]> {
  const loading = i18n.t("common:status.loading");
  await waitFor(() => expect(screen.queryByText(loading)).toBeNull());
  const tabs = screen.getAllByRole("tab");
  for (const tab of tabs) {
    await userEvent.click(tab);
    expect(tab).toHaveAttribute("aria-selected", "true");
    await waitFor(() => expect(screen.queryByText(loading)).toBeNull());
  }
  return tabs.map((tab) => tab.textContent ?? "");
}

describe("M4 screens smoke", () => {
  describe.each(["ar", "fr"] satisfies Lang[])("in %s", (lang) => {
    it("the hardware hub shows every tab in the language", async () => {
      const session: Session = { ...ownerSession, lang };
      await setLanguage(lang);
      missingKeys.clear();
      const unanswered = mockM4(session);
      renderWithProviders(<HardwareHub />);
      const tabs = await clickEveryTab();
      expect(tabs).toEqual(
        ["receipt", "labels", "scale", "displays", "scanners", "drawer"].map((id) =>
          i18n.t(`hardware:hub.tabs.${id}`),
        ),
      );
      await userEvent.click(screen.getByRole("tab", { name: i18n.t("hardware:hub.tabs.scale") }));
      await act(() => emit("weight-changed", { reading: weightFixture }));
      expect(await screen.findByTestId("live-weight")).toHaveTextContent("1,25");
      expect(document.documentElement.dir).toBe(lang === "ar" ? "rtl" : "ltr");
      expect(unanswered(), "every call answered").toEqual([]);
      expect([...missingKeys], `hardware hub in ${lang}`).toEqual([]);
      cleanup();
    }, 30_000);

    it("the labels page shows every tab in the language", async () => {
      // jsdom has no layout: the virtual tables show rows once their scroller measures something
      vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
      vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
      const session: Session = { ...ownerSession, lang };
      await setLanguage(lang);
      missingKeys.clear();
      const unanswered = mockM4(session);
      renderWithProviders(<LabelsPage />, { route: "/products/labels" });
      expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(i18n.t("labels:title"));
      const tabs = await clickEveryTab();
      expect(tabs).toEqual(["templates", "stale", "scale"].map((id) => i18n.t(`labels:tabs.${id}`)));
      const panel = screen.getByRole("tabpanel");
      expect(await within(panel).findByText(lang === "ar" ? "طماطم" : "Tomates")).toBeInTheDocument();
      expect(document.documentElement.dir).toBe(lang === "ar" ? "rtl" : "ltr");
      expect(unanswered(), "every call answered").toEqual([]);
      expect([...missingKeys], `labels page in ${lang}`).toEqual([]);
      cleanup();
    }, 30_000);
  });

  it("the customer screen speaks an Arabic ticket right to left and thanks in three languages", async () => {
    const arabic: DisplayFrame = {
      ...frameFixture,
      lang: "ar",
      mode: "paid",
      paid: 50_000,
      change: 5_000,
      line: { ...frameFixture.line!, name: "حليب كانديا" },
    };
    missingKeys.clear();
    mockCommands({
      display_current: () => arabic,
      display_shop: () => ({
        nameAr: "سوبيرات الأمل",
        nameFr: "Supérette El Amel",
        nameEn: "",
        logo: null,
        showLogo: true,
        thanksSeconds: 5,
      }),
    });
    render(<CustomerDisplay />);
    expect(await screen.findByText("سوبيرات الأمل")).toBeInTheDocument();
    await waitFor(() => expect(document.documentElement.dir).toBe("rtl"));
    expect(document.documentElement.lang).toBe("ar");
    // The thanks wait for the three languages' texts (loaded on first use).
    expect(await screen.findByText("شكراً")).toHaveAttribute("dir", "rtl");
    expect(screen.getByText("Merci")).toHaveAttribute("dir", "ltr");
    expect(screen.getByText("Thank you")).toHaveAttribute("dir", "ltr");
    expect(screen.getByTestId("change")).toHaveTextContent("50,00");
    expect([...missingKeys], "customer screen in ar").toEqual([]);
    cleanup();
    await setLanguage("fr");
  });
});
