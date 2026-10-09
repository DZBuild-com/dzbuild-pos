import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { AboutSettings } from "@/features/settings/AboutSettings";
import { DiagnosticsSettings } from "@/features/settings/DiagnosticsSettings";
import { appInfoFixture, cashierSession, ownerSession, settingsValues } from "@/test/fixtures";
import { mockCommands, renderWithProviders } from "@/test/render";

const info = {
  system: {
    os: "Windows 10 Pro 22H2",
    ramTotalMb: 4_096,
    ramAvailableMb: 1_580,
    processMemoryMb: 212,
    webview2: "141.0.3537.71",
    coldStartMs: 1_840,
    dataDiskFreeMb: 40_960,
    crashDumps: 0,
  },
  lastCrash: {
    at: "2026-09-28 12:00:00.000 Algiers",
    message: "panic in thread 'main': index out of bounds",
  },
  logLevel: "info",
};

describe("Settings › Diagnostic", () => {
  it("shows what support needs, exports the zip and changes the log level", async () => {
    const calls = mockCommands({
      settings_get: ({ key }: Record<string, unknown>) => settingsValues[String(key)],
      diagnostics_info: () => info,
      diagnostics_export: () => ({ entries: ["LISEZ-MOI.txt", "system.json"], bytes: 20_480 }),
      diagnostics_set_log_level: ({ level }: Record<string, unknown>) => level,
      support_contact: () => ({ whatsapp: "213550123456", url: null, label: "Informatique Benali" }),
      support_open: () => null,
    });
    const user = userEvent.setup();
    renderWithProviders(<DiagnosticsSettings session={ownerSession} />);
    expect(await screen.findByText("212 Mo")).toBeInTheDocument();
    expect(screen.getByText(/index out of bounds/)).toBeInTheDocument();
    expect(screen.getByText(/ni vos clients, ni vos ventes/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Exporter le diagnostic (.zip)" }));
    expect(await screen.findByText("Diagnostic enregistré (2 fichiers).")).toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("Détail des journaux techniques"), "debug");
    await waitFor(() => expect(calls).toContain("diagnostics_set_log_level"));
    await user.click(screen.getByRole("button", { name: "Écrire sur WhatsApp à Informatique Benali" }));
    expect(calls).toContain("support_open");
  });

  it("shows the last crash in About to every user, without the diagnostic link for a cashier", async () => {
    mockCommands({
      app_info: () => appInfoFixture,
      app_last_crash: () => info.lastCrash,
      support_contact: () => ({ whatsapp: null, url: "https://dzbuild.com/pos/support", label: null }),
    });
    renderWithProviders(<AboutSettings session={cashierSession} />);
    expect(await screen.findByText(/Dernier plantage/)).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Ouvrir le diagnostic" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ouvrir la page d'assistance" })).toBeInTheDocument();
  });

  it("lets the owner save the support contact, its number cleaned to international digits", async () => {
    const saved: unknown[] = [];
    mockCommands({
      settings_get: ({ key }: Record<string, unknown>) => settingsValues[String(key)],
      settings_set: ({ value }: Record<string, unknown>) => {
        saved.push(value);
        return value;
      },
      diagnostics_info: () => info,
      support_contact: () => ({ whatsapp: null, url: "https://dzbuild.com/pos/support", label: null }),
    });
    const user = userEvent.setup();
    renderWithProviders(<DiagnosticsSettings session={ownerSession} />);
    await user.type(
      await screen.findByLabelText("Nom de votre assistance (revendeur, technicien)"),
      "Informatique Benali",
    );
    await user.type(
      screen.getByLabelText("Numéro WhatsApp (format international, ex. 213550123456)"),
      "+٢١٣ 550 12 34 56",
    );
    await user.click(screen.getByRole("button", { name: "Enregistrer le contact" }));
    expect(await screen.findByText("Contact d'assistance enregistré.")).toBeInTheDocument();
    expect(saved).toEqual([
      { label: "Informatique Benali", whatsapp: "213550123456", url: "https://dzbuild.com/pos/support" },
    ]);
  });
});
