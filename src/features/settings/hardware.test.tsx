import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { HardwareSettings } from "@/features/settings/HardwareSettings";
import { setLanguage } from "@/lib/i18n";
import { mockCommands, renderWithProviders } from "@/test/render";

const empty = {
  version: 1,
  printers: [],
  receiptPrinterId: null,
  receipt: {
    printByDefault: true,
    openDrawerOnCash: true,
    drawerBeforePrint: false,
    logo: true,
    secondLang: null,
  },
  scanner: { layout: "us" },
};

const discovered = [
  {
    kind: "windows_printer",
    name: "XP-80C",
    port: "USB001",
    driver: "XP-80",
    isDefault: false,
    offline: false,
    likelyReceipt: true,
  },
  {
    kind: "windows_printer",
    name: "Microsoft Print to PDF",
    port: "PORTPROMPT:",
    driver: "Microsoft Print To PDF",
    isDefault: true,
    offline: false,
    likelyReceipt: false,
  },
  { kind: "serial_port", port: "COM3", description: "USB-SERIAL CH340" },
];

describe("HardwareSettings", () => {
  it("detects a receipt printer, prints a test page and saves it as the receipt printer", async () => {
    const saved: unknown[] = [];
    const tests: unknown[] = [];
    mockCommands({
      hardware_get: () => empty,
      hardware_discover: () => discovered,
      hardware_test_print: (args) => {
        tests.push(args);
        return { printer: "Caisse 1", bytes: 4_000, raster: true };
      },
      hardware_save: ({ config }) => {
        saved.push(config);
        return config;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<HardwareSettings />);
    await user.click(await screen.findByRole("button", { name: "Ajouter une imprimante" }));
    const dialog = await screen.findByRole("dialog", { name: "Imprimante" });
    await user.click(within(dialog).getByRole("button", { name: "Détecter les imprimantes" }));
    expect(
      await within(dialog).findByRole("option", { name: "XP-80C (imprimante de tickets probable)" }),
    ).toBeInTheDocument();
    await user.selectOptions(within(dialog).getByLabelText("Imprimante Windows"), "XP-80C");
    await user.click(within(dialog).getByRole("radio", { name: "58 mm" }));
    await user.click(within(dialog).getByRole("button", { name: "Imprimer une page de test" }));
    await waitFor(() => expect(tests).toHaveLength(1));
    expect(tests[0]).toMatchObject({
      printer: {
        transport: { kind: "windows", printerName: "XP-80C" },
        profile: { width: "mm58", dots: 384, columns: 32 },
      },
      lang: "fr",
    });
    expect(await within(dialog).findByText("Page de test envoyée à Caisse 1.")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer l'imprimante" }));
    await user.click(screen.getByRole("button", { name: "Enregistrer les réglages du matériel" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({
      receiptPrinterId: expect.any(String),
      printers: [{ name: "Caisse 1", transport: { printerName: "XP-80C" } }],
    });
  });

  it("sets up a network printer and refuses a bad port", async () => {
    mockCommands({ hardware_get: () => empty });
    const user = userEvent.setup();
    renderWithProviders(<HardwareSettings />);
    await user.click(await screen.findByRole("button", { name: "Ajouter une imprimante" }));
    const dialog = await screen.findByRole("dialog", { name: "Imprimante" });
    await user.click(within(dialog).getByRole("radio", { name: "Réseau (adresse IP)" }));
    await user.type(within(dialog).getByLabelText("Adresse IP"), "192.168.1.50");
    await user.clear(within(dialog).getByLabelText("Port"));
    await user.type(within(dialog).getByLabelText("Port"), "0");
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer l'imprimante" }));
    expect(
      await within(dialog).findByText("Saisissez un port entre 1 et 65535 (souvent 9100)."),
    ).toBeInTheDocument();
  });

  it("tests the scanner in the chosen layout", async () => {
    mockCommands({ hardware_get: () => ({ ...empty, scanner: { layout: "azerty" } }) });
    renderWithProviders(<HardwareSettings />);
    const box = await screen.findByLabelText("Testez votre scanner ici");
    box.focus();
    for (const ch of "6130000000017")
      fireEvent.keyDown(box, { code: `Digit${ch}`, key: "&", shiftKey: true });
    fireEvent.keyDown(box, { code: "Enter", key: "Enter" });
    expect(await screen.findByText("Code lu : 6130000000017")).toBeInTheDocument();
  });

  it("reads right to left in Arabic", async () => {
    await setLanguage("ar");
    mockCommands({ hardware_get: () => empty });
    renderWithProviders(<HardwareSettings />);
    expect(await screen.findByRole("button", { name: "إضافة طابعة" })).toBeInTheDocument();
    expect(document.documentElement).toHaveAttribute("dir", "rtl");
  });
});
