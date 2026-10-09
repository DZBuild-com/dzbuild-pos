import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router";
import { describe, expect, it } from "vitest";
import { AboutSettings } from "@/features/settings/AboutSettings";
import { appInfoFixture, cashierSession, ownerSession } from "@/test/fixtures";
import { mockCommands, renderWithProviders } from "@/test/render";

function mockAbout() {
  const opened: unknown[] = [];
  mockCommands({
    app_info: () => appInfoFixture,
    app_last_crash: () => null,
    support_contact: () => ({ whatsapp: null, url: null, label: null }),
    app_open_link: ({ kind }: Record<string, unknown>) => {
      opened.push(kind);
      return null;
    },
  });
  return opened;
}

describe("Settings › About (free edition)", () => {
  it("says the POS is free, with no word of a licence to activate", async () => {
    mockAbout();
    renderWithProviders(<AboutSettings session={ownerSession} />);
    expect(
      await screen.findByText(
        "DZBuild POS est gratuit : pas de licence, pas de clé, pas d'abonnement. Les mises à jour sont gratuites aussi.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Réglages › Licence/)).toBeNull();
    expect(screen.queryByText(/essai|activer/i)).toBeNull();
  });

  it("opens the DZBuild sign-up and the blog through the app's fixed links", async () => {
    const opened = mockAbout();
    const user = userEvent.setup();
    renderWithProviders(<AboutSettings session={ownerSession} />);
    expect(
      await screen.findByRole("heading", { name: "Développez votre commerce avec DZBuild" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Créez votre site et vendez dans toute l'Algérie.")).toBeInTheDocument();
    expect(screen.getByText("Conseils et guides pour les commerçants.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Créer mon site" }));
    await waitFor(() => expect(opened).toEqual(["dzbuild_register"]));
    await user.click(screen.getByRole("button", { name: "Lire le blog" }));
    await waitFor(() => expect(opened).toEqual(["dzbuild_register", "dzbuild_blog"]));
  });

  it("takes the owner to Settings › DZBuild to link the shop", async () => {
    mockAbout();
    const user = userEvent.setup();
    renderWithProviders(
      <Routes>
        <Route path="/settings/about" element={<AboutSettings session={ownerSession} />} />
        <Route path="/settings/dzbuild" element={<p>DZBuild page</p>} />
      </Routes>,
      { route: "/settings/about" },
    );
    expect(
      await screen.findByText(
        "Liez ce magasin à votre compte DZBuild pour publier votre catalogue et recevoir les commandes en ligne.",
      ),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("link", { name: "Ouvrir la liaison DZBuild" }));
    expect(await screen.findByText("DZBuild page")).toBeInTheDocument();
  });

  it("shows a cashier the website and the blog, but not the link they cannot open", async () => {
    mockAbout();
    renderWithProviders(<AboutSettings session={cashierSession} />);
    expect(await screen.findByRole("button", { name: "Créer mon site" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Lire le blog" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Ouvrir la liaison DZBuild" })).toBeNull();
  });
});
