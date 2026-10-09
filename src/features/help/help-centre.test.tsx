import { fireEvent, screen } from "@testing-library/react";
import { useLocation } from "react-router";
import { describe, expect, it } from "vitest";
import { HelpCentre } from "@/features/help/HelpCentre";
import { HelpDialog } from "@/features/pos/HelpDialog";
import { setLanguage } from "@/lib/i18n";
import { renderWithProviders } from "@/test/render";

describe("help centre index", () => {
  it("lists the pages by group and finds them by their title, accents aside", async () => {
    renderWithProviders(<HelpCentre />, { route: "/help" });
    expect(await screen.findByRole("link", { name: "Premiers pas" })).toHaveAttribute(
      "href",
      "/help/first-day",
    );
    expect(screen.getByRole("heading", { level: 2, name: "Sécurité des données" })).toBeInTheDocument();
    const search = screen.getByLabelText("Rechercher dans l'aide");
    fireEvent.change(search, { target: { value: "sauvegarde" } });
    expect(screen.getByRole("link", { name: "Sauvegardes" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Restaurer une sauvegarde" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Vendre" })).toBeNull();
    fireEvent.change(search, { target: { value: "demonstration" } });
    expect(screen.getByRole("link", { name: "Boutique de démonstration" })).toBeInTheDocument();
    fireEvent.change(search, { target: { value: "zzz" } });
    expect(screen.getByRole("status")).toHaveTextContent("Aucune page ne correspond.");
  });

  it("shows the titles in the UI language", async () => {
    await setLanguage("ar");
    renderWithProviders(<HelpCentre />, { route: "/help" });
    expect(await screen.findByRole("link", { name: "النسخ الاحتياطية" })).toHaveAttribute(
      "href",
      "/help/settings-backups",
    );
    expect(screen.getByRole("heading", { level: 1, name: "مركز المساعدة" })).toBeInTheDocument();
  });

  it("the till's F1 dialog opens the full help of the sale screen", () => {
    let path = "";
    function Where() {
      path = useLocation().pathname;
      return null;
    }
    let closed = 0;
    renderWithProviders(
      <>
        <HelpDialog onClose={() => (closed += 1)} />
        <Where />
      </>,
      { route: "/pos" },
    );
    fireEvent.click(screen.getByRole("button", { name: "Ouvrir l'aide complète" }));
    expect(closed).toBe(1);
    expect(path).toBe("/help/pos");
  });
});
