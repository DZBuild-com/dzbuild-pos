import { emit } from "@tauri-apps/api/event";
import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { UpdatesSettings } from "@/features/settings/UpdatesSettings";
import { UpdateAfterClose } from "@/features/updates/UpdateAfterClose";
import { WhatsNewDialog } from "@/features/updates/WhatsNewDialog";
import { setLanguage } from "@/lib/i18n";
import { ownerSession, settingsValues } from "@/test/fixtures";
import { lanTillStatusFixture } from "@/test/fixtures-m5";
import { mockCommands, renderWithProviders } from "@/test/render";

const offer = {
  version: "0.6.1",
  notes: {
    ar: "تصحيح طباعة التذاكر العربية",
    fr: "Correction de l'impression des tickets en arabe",
    en: "Fix for Arabic receipt printing",
  },
  pubDate: "2026-10-15T09:00:00Z",
  decision: { kind: "offer" },
};

const status = (over: Record<string, unknown> = {}) => ({
  currentVersion: "0.6.0",
  channel: "stable",
  autoCheck: true,
  autoDownload: true,
  lastCheckAt: 1_790_593_200_000,
  lastCheckError: null,
  offer,
  download: { kind: "idle" },
  blockers: [],
  installing: false,
  ...over,
});

const settings = {
  settings_get: ({ key }: Record<string, unknown>) => settingsValues[String(key)],
  settings_set: ({ value }: Record<string, unknown>) => value,
};

describe("updates", () => {
  it("is read-only on a till: the channel and the automatic switches are the main PC's", async () => {
    const saved: unknown[] = [];
    mockCommands({
      settings_get: ({ key }: Record<string, unknown>) => settingsValues[String(key)],
      settings_set: (args: Record<string, unknown>) => {
        saved.push(args);
        return args.value;
      },
      updates_status: () => status(),
      lan_status: () => lanTillStatusFixture,
    });
    renderWithProviders(<UpdatesSettings session={ownerSession} />);
    expect(
      await screen.findByText(
        "Lecture seule sur ce poste de caisse : faites-le sur le poste principal (Supérette El Amel).",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Rechercher les mises à jour chaque jour" })).toBeDisabled();
    expect(screen.getByRole("radio", { name: /Stable/ })).toBeDisabled();
    expect(saved).toEqual([]);
  });

  it("the install button is disabled with the reason while the drawer is open", async () => {
    mockCommands({
      ...settings,
      updates_status: () =>
        status({
          download: { kind: "ready" },
          blockers: [
            { kind: "cashSessionOpen", terminalCode: "C1", userName: "Yacine", openedAt: 1_790_578_800_000 },
          ],
        }),
    });
    renderWithProviders(<UpdatesSettings session={ownerSession} />);
    const install = await screen.findByRole("button", { name: "Installer maintenant" });
    expect(install).toBeDisabled();
    expect(screen.getByText(/La caisse C1 est ouverte \(par Yacine/)).toBeInTheDocument();
    expect(screen.getByText("Correction de l'impression des tickets en arabe")).toBeInTheDocument();
  });

  it("downloads with progress, then installs only after the confirmation", async () => {
    let current = status();
    const calls = mockCommands({
      ...settings,
      updates_status: () => current,
      updates_download: () => {
        current = status({ download: { kind: "downloading", received: 0, total: 8_000_000 } });
        return current;
      },
      updates_install: () => null,
    });
    const user = userEvent.setup();
    renderWithProviders(<UpdatesSettings session={ownerSession} />);
    await user.click(await screen.findByRole("button", { name: "Télécharger" }));
    await act(async () => {
      await emit("update-progress", { received: 4_000_000, total: 8_000_000 });
    });
    expect(await screen.findByRole("progressbar")).toHaveAttribute("aria-valuenow", "50");
    current = status({ download: { kind: "ready" } });
    await act(async () => {
      await emit("update-progress", { received: 8_000_000, total: 8_000_000 });
    });
    await user.click(await screen.findByRole("button", { name: "Installer maintenant" }));
    expect(calls).not.toContain("updates_install");
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Installer et redémarrer" }));
    await waitFor(() => expect(calls).toContain("updates_install"));
  });

  it("closing the Z offers the update and installs only on confirmation", async () => {
    const calls = mockCommands({
      updates_status: () => status({ download: { kind: "ready" } }),
      updates_install: () => null,
    });
    const user = userEvent.setup();
    renderWithProviders(<UpdateAfterClose session={ownerSession} />);
    expect(
      await screen.findByText(
        "La version 0.6.1 est prête. L'installer maintenant ? DZBuild POS redémarrera.",
      ),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Plus tard" }));
    expect(calls).not.toContain("updates_install");
    expect(screen.queryByText(/est prête/)).not.toBeInTheDocument();
  });

  it("a paused release says so and offers nothing to install", async () => {
    mockCommands({
      ...settings,
      updates_status: () => status({ offer: { ...offer, decision: { kind: "paused" } } }),
    });
    renderWithProviders(<UpdatesSettings session={ownerSession} />);
    expect(
      await screen.findByText("DZBuild a mis cette version en pause : rien à faire, continuez à travailler."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Télécharger" })).not.toBeInTheDocument();
  });

  it("shows the notes and what's new in Arabic", async () => {
    await setLanguage("ar");
    mockCommands({ ...settings, updates_status: () => status() });
    renderWithProviders(<UpdatesSettings session={ownerSession} />);
    expect(await screen.findByText("تصحيح طباعة التذاكر العربية")).toBeInTheDocument();
    renderWithProviders(<WhatsNewDialog version="0.7.0" onClose={() => undefined} />);
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(within(screen.getByRole("dialog")).getAllByRole("listitem").length).toBeGreaterThan(3);
  });
});
