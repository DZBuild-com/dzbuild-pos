import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { DemoRibbon } from "@/app/DemoRibbon";
import { LoginScreen } from "@/features/auth/LoginScreen";
import { DemoCard } from "@/features/settings/DemoCard";
import { SetupWizard } from "@/features/setup/SetupWizard";
import { ownerTile, packFixture, settingsValues } from "@/test/fixtures";
import { mockCommands, renderWithProviders } from "@/test/render";

describe("demo and first screen", () => {
  it("offers the demo, the import from another PC and the tour before anything else", async () => {
    const calls = mockCommands({
      datapack_get: () => packFixture,
      settings_get: ({ key }: Record<string, unknown>) => settingsValues[String(key)],
      demo_enter: () => null,
      setup_import_backup: () => false,
      support_contact: () => ({ whatsapp: null, url: null, label: null }),
    });
    const user = userEvent.setup();
    renderWithProviders(<SetupWizard />);
    await user.click(await screen.findByRole("button", { name: "Regarder la visite (2 minutes)" }));
    const tour = await screen.findByRole("dialog");
    expect(
      await within(tour).findByRole("heading", { level: 1, name: "La visite en 2 minutes" }),
    ).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await user.click(
      screen.getByRole("button", { name: "Transférer ma boutique depuis un autre PC (fichier .dzpos)" }),
    );
    await waitFor(() => expect(calls).toContain("setup_import_backup"));
    await user.click(screen.getByRole("button", { name: "Découvrir avec une boutique de démonstration" }));
    await waitFor(() => expect(calls).toContain("demo_enter"));
    expect(await screen.findByText("Ouverture de la démonstration…")).toBeInTheDocument();
  });

  it("marks every screen of the demo and leaves it", async () => {
    const calls = mockCommands({
      demo_status: () => ({ active: true, exists: true, ownerPin: "2580", cashierPin: "4826" }),
      demo_leave: () => null,
    });
    const user = userEvent.setup();
    renderWithProviders(<DemoRibbon />);
    expect(await screen.findByText("Boutique de démonstration — rien ici n'est réel.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Quitter la démo" }));
    await waitFor(() => expect(calls).toContain("demo_leave"));
  });

  it("shows the demo PINs on the login screen, each in its own ltr island", async () => {
    mockCommands({
      auth_list_users: () => [ownerTile],
      company_get: () => null,
      settings_get: ({ key }: Record<string, unknown>) => settingsValues[String(key)],
      demo_status: () => ({ active: true, exists: true, ownerPin: "2580", cashierPin: "4826" }),
    });
    renderWithProviders(<LoginScreen />);
    const hint = await screen.findByText(
      (_, element) =>
        element?.tagName === "P" && element.textContent === "Démo : propriétaire 2580, caissier 4826",
    );
    expect(within(hint).getByText("2580")).toHaveAttribute("dir", "ltr");
    expect(within(hint).getByText("4826")).toHaveAttribute("dir", "ltr");
    expect(screen.getByRole("note")).toHaveTextContent("Boutique de démonstration — rien ici n'est réel.");
  });

  it("lets the owner erase the demo, and open it only after the confirmation", async () => {
    const entered: unknown[] = [];
    const calls = mockCommands({
      demo_status: () => ({ active: false, exists: true, ownerPin: null, cashierPin: null }),
      demo_enter: ({ lang }: Record<string, unknown>) => {
        entered.push(lang);
        return null;
      },
      demo_erase: () => null,
    });
    const user = userEvent.setup();
    renderWithProviders(<DemoCard />);
    await user.click(await screen.findByRole("button", { name: "Effacer la démo" }));
    await waitFor(() => expect(calls).toContain("demo_erase"));
    await user.click(screen.getByRole("button", { name: "Ouvrir la boutique de démonstration" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/Vos vraies données ne sont pas touchées/)).toBeInTheDocument();
    expect(calls).not.toContain("demo_enter");
    await user.click(within(dialog).getByRole("button", { name: "Ouvrir la boutique de démonstration" }));
    await waitFor(() => expect(entered).toEqual(["fr"]));
    expect(await screen.findByText("Ouverture de la démonstration…")).toBeInTheDocument();
  });
});
