import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "@/app/App";
import { rememberSetupLang } from "@/features/setup/setup-lang";
import { i18n, setLanguage } from "@/lib/i18n";
import type { LicenseStatus } from "@/lib/ipc/types";
import { packFixture, settingsValues } from "@/test/fixtures";
import { DEVICE_CODE, activeStatus, otherDeviceStatus, unlicensedStatus } from "@/test/fixtures-m9";
import { missingKeys, mockCommands, renderWithProviders } from "@/test/render";
import { ActivationScreen } from "./ActivationScreen";

async function inFrench() {
  rememberSetupLang("fr");
  await setLanguage("fr");
}

describe.each(["ar", "fr", "en"] as const)("the activation screen in %s", (lang) => {
  it("shows this PC's code left to right, its QR and every text in the language", async () => {
    mockCommands({});
    rememberSetupLang(lang);
    await setLanguage(lang);
    missingKeys.clear();
    renderWithProviders(<ActivationScreen status={unlicensedStatus} />);
    const code = await screen.findByTestId("device-code");
    expect(code).toHaveTextContent(DEVICE_CODE);
    expect(code).toHaveAttribute("dir", "ltr");
    expect(screen.getByRole("img", { name: i18n.t("license:device.qr") })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(i18n.t("license:activation.title"));
    expect(screen.getByLabelText(i18n.t("license:form.key"))).toBeInTheDocument();
    expect(document.documentElement.dir).toBe(lang === "ar" ? "rtl" : "ltr");
    expect([...missingKeys]).toEqual([]);
  });
});

describe("the activation screen", () => {
  it("activates a pasted key on a new install, then the setup wizard opens", async () => {
    await inFrench();
    let current: LicenseStatus = unlicensedStatus;
    const keys: unknown[] = [];
    mockCommands({
      app_startup_error: () => null,
      app_startup_notices: () => [],
      setup_status: () => ({ completed: false, hasOwner: false, terminalCode: "C1", loginLang: "fr" }),
      settings_get: ({ key }) => settingsValues[String(key)],
      datapack_get: () => packFixture,
      license_status: () => current,
      license_activate: ({ key }) => {
        keys.push(key);
        current = activeStatus;
        return current;
      },
    });
    renderWithProviders(<App />);
    expect(await screen.findByRole("heading", { name: "Activer DZBuild POS" })).toBeInTheDocument();
    expect(screen.queryByText("Installation de DZBuild POS")).not.toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Clé de licence"), "  DZL1.abc.def  ");
    await userEvent.click(screen.getByRole("button", { name: "Activer la licence" }));
    expect(await screen.findByRole("heading", { name: "Installation de DZBuild POS" })).toBeInTheDocument();
    expect(keys).toEqual(["DZL1.abc.def"]);
  });

  it("says why a key is refused, in the user's words", async () => {
    await inFrench();
    mockCommands({ license_activate: () => Promise.reject({ code: "license_wrong_device" }) });
    renderWithProviders(<ActivationScreen status={unlicensedStatus} />);
    await userEvent.type(await screen.findByLabelText("Clé de licence"), "DZL1.abc.def");
    await userEvent.click(screen.getByRole("button", { name: "Activer la licence" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("délivrée pour un autre PC");
  });

  it("copies the code, loads a .dzlic through Rust's dialog and offers the demo shop", async () => {
    await inFrench();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    const calls = mockCommands({ license_import_file: () => null, demo_enter: () => null });
    renderWithProviders(<ActivationScreen status={unlicensedStatus} />);
    await userEvent.click(await screen.findByRole("button", { name: "Copier le code" }));
    expect(writeText).toHaveBeenCalledWith(DEVICE_CODE);
    await userEvent.click(screen.getByRole("button", { name: "Charger un fichier .dzlic" }));
    await waitFor(() => expect(calls).toContain("license_import_file"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Essayer d'abord la boutique de démonstration" }),
    );
    await waitFor(() => expect(calls).toContain("demo_enter"));
  });

  it("explains a PC that changed and names the previous one", async () => {
    await inFrench();
    mockCommands({});
    renderWithProviders(<ActivationScreen status={otherDeviceStatus} />);
    const notice = await screen.findByRole("alert");
    expect(notice).toHaveTextContent("appartient à un autre PC");
    expect(notice).toHaveTextContent("DZPOS-0A1B-2C3D-4E5F-6G73");
  });

  it("uses logical CSS properties only", () => {
    const dir = path.resolve(import.meta.dirname);
    for (const file of readdirSync(dir).filter((f) => f.endsWith(".tsx") && !f.endsWith(".test.tsx"))) {
      const text = readFileSync(path.join(dir, file), "utf8");
      expect(text, file).not.toMatch(/\b(ml|mr|pl|pr|left|right)-[\w[]|\btext-(left|right)\b/);
    }
  });
});
