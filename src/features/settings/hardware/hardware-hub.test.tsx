import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { emit } from "@tauri-apps/api/event";
import { describe, expect, it } from "vitest";
import { AppRoutes } from "@/app/routes";
import { DeviceDot } from "@/features/settings/hardware/DeviceDot";
import { HardwareHub } from "@/features/settings/hardware/HardwareHub";
import { presetProfile } from "@/features/settings/PrinterForm";
import { setLanguage } from "@/lib/i18n";
import { ownerSession } from "@/test/fixtures";
import { hardwareV2Fixture, weightFixture } from "@/test/fixtures-m4";
import { packState } from "@/test/fixtures-m8";
import { activeStatus } from "@/test/fixtures-m9";
import { mockCommands, renderWithProviders } from "@/test/render";

const discovered = [
  {
    kind: "windows_printer",
    name: "Xprinter XP-365B",
    port: "USB002",
    driver: "XP-365B",
    isDefault: false,
    offline: false,
    likelyReceipt: false,
    likelyLabel: true,
  },
  { kind: "serial_port", port: "COM3", description: "USB-SERIAL CH340" },
];

function mockHub(extra: Record<string, (args: Record<string, unknown>) => unknown> = {}) {
  const saved: unknown[] = [];
  const calls = mockCommands({
    hardware_get: () => hardwareV2Fixture,
    // M9: the shell's licence banner (a lifetime licence: no banner)
    license_status: () => activeStatus,
    hardware_save: (args) => {
      saved.push(args.config);
      return args.config;
    },
    hardware_discover: () => discovered,
    devices_status: () => [
      { device: "scale", state: { state: "off" } },
      { device: "vfd", state: { state: "off" } },
      { device: "serial_scanner", state: { state: "off" } },
    ],
    datapack_get: () => ({
      scaleBarcodes: {
        source: "",
        defaultLayouts: [
          { prefix: "21", pluDigits: 5, verifier: false, value: "weight", valueDigits: 5, decimals: 3 },
        ],
      },
    }),
    settings_get: (args) =>
      ({ "scale.barcode_formats": null, "products.internal_ean_prefix": "200" })[args.key as string],
    display_monitors: () => [
      { name: "\\\\.\\DISPLAY1", width: 1366, height: 768, x: 0, y: 0, primary: true, mainWindow: true },
      { name: "\\\\.\\DISPLAY2", width: 1024, height: 768, x: 1366, y: 0, primary: false, mainWindow: false },
    ],
    scale_latest: () => weightFixture,
    ...extra,
  });
  return { saved, calls };
}

/** Two receipt printers on this PC: the till's and the kitchen's network printer. */
const twoPrinters = {
  ...hardwareV2Fixture,
  printers: [
    {
      id: "p-caisse",
      name: "Caisse",
      transport: { kind: "windows" as const, printerName: "XP-80C" },
      profile: presetProfile("generic", "mm80"),
      mode: "raw" as const,
      paperWidthMm: 80,
    },
    {
      id: "p-cuisine",
      name: "Cuisine",
      transport: { kind: "tcp" as const, host: "192.168.1.50", port: 9100 },
      profile: presetProfile("generic", "mm80"),
      mode: "raw" as const,
      paperWidthMm: 80,
    },
  ],
  receiptPrinterId: "p-caisse",
};

const station = (id: string, code: string, nameFr: string) => ({
  id,
  code,
  nameAr: code,
  nameFr,
  nameEn: nameFr,
  sortOrder: 0,
  archived: false,
  categoryIds: [],
});

describe("kitchen printers and Windows-driver printers", () => {
  it("maps each preparation station to a printer, with its copies and buzzer, in a restaurant", async () => {
    const { saved } = mockHub({
      hardware_get: () => twoPrinters,
      packs_state: () => packState({ modules: ["restaurant"] }),
      kitchen_stations: () => [station("s-grill", "GRILL", "Grill"), station("s-bar", "BAR", "Bar")],
    });
    renderWithProviders(<HardwareHub />);
    await userEvent.click(await screen.findByRole("tab", { name: "Cuisine" }));
    const grill = await screen.findByRole("group", { name: "Grill" });
    await userEvent.selectOptions(within(grill).getByLabelText("Imprimante du poste"), "p-cuisine");
    await userEvent.selectOptions(within(grill).getByLabelText("Copies"), "2");
    await userEvent.click(within(grill).getByRole("checkbox", { name: "Sonnerie après chaque bon" }));
    await userEvent.selectOptions(screen.getByLabelText("Imprimante de secours"), "p-cuisine");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les réglages du matériel" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect((saved[0] as typeof twoPrinters & { kitchen: unknown }).kitchen).toEqual({
      stations: [{ stationId: "s-grill", printerId: "p-cuisine", copies: 2, buzzer: true }],
      fallbackPrinterId: "p-cuisine",
    });
  });

  it("has no kitchen tab without the restaurant module", async () => {
    mockHub({ packs_state: () => packState({ modules: [] }) });
    renderWithProviders(<HardwareHub />);
    expect(await screen.findByRole("tab", { name: "Étiquettes" })).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Cuisine" })).toBeNull();
  });

  it("prints a receipt printer that refuses raw data through its Windows driver on a 58 mm roll", async () => {
    const { saved } = mockHub({ packs_state: () => packState({ modules: [] }) });
    renderWithProviders(<HardwareHub />);
    await userEvent.click(await screen.findByRole("button", { name: "Ajouter une imprimante" }));
    await userEvent.type(screen.getByLabelText("Imprimante Windows"), "HP LaserJet");
    const mode = screen.getByRole("group", { name: "Mode d'impression" });
    await userEvent.click(within(mode).getByRole("radio", { name: "Pilote Windows (pages)" }));
    const roll = screen.getByRole("group", { name: "Largeur du rouleau (pages)" });
    await userEvent.click(within(roll).getByRole("radio", { name: "58 mm" }));
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer l'imprimante" }));
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les réglages du matériel" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect((saved[0] as typeof twoPrinters).printers[0]).toMatchObject({
      transport: { kind: "windows", printerName: "HP LaserJet" },
      mode: "driver",
      paperWidthMm: 58,
    });
  });
});

describe("hardware hub", () => {
  it("adds a label printer from discovery and saves one hardware.json", async () => {
    const { saved } = mockHub({
      hardware_test_label: () => ({ printer: "Étiquettes", bytes: 100, raster: true }),
    });
    renderWithProviders(<HardwareHub />);
    await userEvent.click(await screen.findByRole("tab", { name: "Étiquettes" }));
    await userEvent.click(screen.getByRole("button", { name: "Ajouter une imprimante d'étiquettes" }));
    await userEvent.click(screen.getByRole("button", { name: "Détecter" }));
    await userEvent.click(await screen.findByRole("button", { name: /Xprinter XP-365B/ }));
    await userEvent.click(screen.getByRole("button", { name: "Imprimer une étiquette de test" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Étiquettes");
    await userEvent.click(screen.getByRole("button", { name: "Garder cette imprimante" }));
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les réglages du matériel" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    const config = saved[0] as typeof hardwareV2Fixture;
    expect(config.labelPrinters?.[0]?.profile.language).toBe("tspl");
    expect(config.labelPrinterId).toBe(config.labelPrinters?.[0]?.id);
  });

  it("detects a scale on its port and shows its live weight", async () => {
    mockHub({
      scale_probe: () => ({
        protocol: "toledo8217",
        serial: { port: "COM3", baud: 9600, dataBits: "seven", parity: "even", stopBits: "one" },
      }),
    });
    renderWithProviders(<HardwareHub />);
    await userEvent.click(await screen.findByRole("tab", { name: "Balance" }));
    await userEvent.selectOptions(screen.getByLabelText("Port de la balance"), "COM3");
    await userEvent.click(screen.getByRole("button", { name: "Détecter la balance" }));
    expect(await screen.findByDisplayValue("Mettler Toledo 8217")).toBeInTheDocument();
    await act(() =>
      emit("weight-changed", { reading: { ...weightFixture, gross: 850, net: 850, stable: true } }),
    );
    const live = await screen.findByTestId("live-weight");
    expect(live).toHaveTextContent("0,85");
    expect(within(live).getByText("Stable")).toBeInTheDocument();
  });

  it("offers only the monitors that are not the till's own", async () => {
    mockHub();
    renderWithProviders(<HardwareHub />);
    await userEvent.click(await screen.findByRole("tab", { name: "Afficheurs" }));
    await userEvent.click(screen.getByLabelText("Afficher l'écran client"));
    expect(await screen.findByRole("radio", { name: /DISPLAY1/ })).toBeDisabled();
    expect(screen.getByRole("radio", { name: /DISPLAY2/ })).toBeEnabled();
  });

  it("shows a pole display ticked without its port on its own tab and field, and saves nothing", async () => {
    const { saved } = mockHub();
    renderWithProviders(<HardwareHub />);
    await userEvent.click(await screen.findByRole("tab", { name: "Afficheurs" }));
    await userEvent.click(screen.getByLabelText("Utiliser un afficheur sur port série"));
    await userEvent.click(screen.getByRole("tab", { name: "Lecteurs" }));
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les réglages du matériel" }));
    expect(screen.getByRole("tab", { name: "Afficheurs" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByLabelText("Port COM").closest("div")).toHaveTextContent(
      "Choisissez le port COM de cet appareil.",
    );
    expect(saved).toHaveLength(0);
  });

  it("opens the tab of a field the backend refuses", async () => {
    mockHub({
      hardware_save: () =>
        Promise.reject({ code: "hardware_setting_invalid", field: "drawer.port", message: "drawer.port" }),
    });
    renderWithProviders(<HardwareHub />);
    await userEvent.click(await screen.findByRole("tab", { name: "Balance" }));
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les réglages du matériel" }));
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Tiroir" })).toHaveAttribute("aria-selected", "true"),
    );
  });

  it("warns when a weight barcode prefix starts the in-store codes", async () => {
    mockHub({
      settings_get: (args) =>
        ({ "scale.barcode_formats": null, "products.internal_ean_prefix": "210" })[args.key as string],
    });
    renderWithProviders(<HardwareHub />);
    await userEvent.click(await screen.findByRole("tab", { name: "Balance" }));
    expect(
      await screen.findByText(/Le préfixe 21 est aussi le début de vos codes internes/),
    ).toBeInTheDocument();
  });

  it("explains a swapped driver in plain words, in Arabic too", async () => {
    mockHub({
      hardware_doctor: () => [
        {
          printerId: "p1",
          printerName: "XP-80C",
          issue: { kind: "driver_changed", saved: "XP-80", now: "Generic / Text Only" },
        },
      ],
    });
    await setLanguage("ar");
    renderWithProviders(<HardwareHub />);
    await userEvent.click(await screen.findByRole("button", { name: "فحص الطابعات" }));
    expect(await screen.findByText(/XP-80C/)).toBeInTheDocument();
    expect(screen.getByText(/Generic \/ Text Only/)).toBeInTheDocument();
    await setLanguage("fr");
  });

  it("learns an AZERTY scanner and its AIM prefix from one scan of the test page", async () => {
    const { saved } = mockHub();
    renderWithProviders(<HardwareHub />);
    await userEvent.click(await screen.findByRole("tab", { name: "Lecteurs" }));
    await userEvent.click(screen.getByRole("button", { name: "Apprendre mon lecteur" }));
    const capture = await screen.findByLabelText("Cliquez ici puis scannez");
    // "]E0DZPOS-TEST-0123" typed by a scanner emulating AZERTY (shifted digits, W for Z), then Enter
    const keys: [string, boolean][] = [
      ["BracketRight", false],
      ["KeyE", true],
      ["Digit0", true],
      ...[..."DZPOS"].map((l): [string, boolean] => [l === "Z" ? "KeyW" : `Key${l}`, true]),
      ["Digit6", false],
      ...[..."TEST"].map((l): [string, boolean] => [`Key${l}`, true]),
      ["Digit6", false],
      ...[..."0123"].map((d): [string, boolean] => [`Digit${d}`, true]),
    ];
    for (const [code, shiftKey] of keys) fireEvent.keyDown(capture, { code, key: "x", shiftKey });
    fireEvent.keyDown(capture, { code: "Enter", key: "Enter" });
    expect(await screen.findByText(/Lecteur reconnu/)).toHaveTextContent("AZERTY français");
    await userEvent.click(screen.getByRole("button", { name: "Appliquer" }));
    expect(screen.getByLabelText("Préfixe envoyé par le lecteur")).toHaveValue("]E0");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les réglages du matériel" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect((saved[0] as typeof hardwareV2Fixture).scanner).toMatchObject({
      layout: "azerty",
      prefix: "]E0",
      suffix: "enter_or_tab",
    });
  });

  it("opens the document settings, where the Arabic-label rule lives, from the label printers tab", async () => {
    mockHub({
      settings_get: () => null,
      categories_list: () => [],
    });
    renderWithProviders(<AppRoutes session={ownerSession} />, { route: "/settings/hardware" });
    await userEvent.click(await screen.findByRole("tab", { name: "Étiquettes" }));
    await userEvent.click(screen.getByRole("link", { name: "Ouvrir les réglages des documents" }));
    // The M3 screen holds `labels.arabic_categories` next to `documents.arabic_guard` (DOC-034).
    expect(
      await screen.findByText("Arabe sur les étiquettes et les documents", { selector: "h2" }),
    ).toBeInTheDocument();
  });

  it("says why a device dot is red", async () => {
    mockHub({
      devices_status: () => [
        { device: "scale", state: { state: "disconnected", port: "COM3", code: "scale_not_responding" } },
      ],
    });
    renderWithProviders(<DeviceDot device="scale" />);
    expect(
      await screen.findByRole("img", { name: /^Balance : Débranché \(COM3\) : La balance ne répond pas/ }),
    ).toBeInTheDocument();
  });
});
