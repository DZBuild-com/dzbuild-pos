import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { FatalScreen } from "@/app/FatalScreen";
import { mockCommands, renderWithProviders } from "@/test/render";

/**
 * Task 11 review: a till offers to set its copy aside and join its main PC again only
 * when that copy cannot be used. Any other failure keeps M0's screen: the files stay in place,
 * because a retry or the latest version may open them and they may hold sales the main PC never
 * received.
 */
const tillFailure = (code: string) => ({
  error: { code },
  dataDir: "C:\\Users\\amine\\AppData\\Roaming\\dev.dzbuild.pos",
  tillOf: "Supérette El Amel",
});

const rejoin = "Rejoindre de nouveau le poste principal";
const restore = "Restaurer une sauvegarde";
const openFolder = "Ouvrir le dossier des données";

describe("a till's fatal screen offers to join again only for a copy it cannot use", () => {
  it.each(["database_corrupt", "database_missing", "restore_needs_attention"])("%s", (code) => {
    mockCommands({});
    renderWithProviders(<FatalScreen failure={tillFailure(code)} />);
    expect(screen.getByRole("button", { name: rejoin })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: restore })).not.toBeInTheDocument();
  });

  it("says what the damaged copy had not sent before the owner confirms (M6 review)", async () => {
    mockCommands({});
    const user = userEvent.setup();
    renderWithProviders(<FatalScreen failure={{ ...tillFailure("database_corrupt"), unsent: 3 }} />);
    await user.click(screen.getByRole("button", { name: rejoin }));
    expect(
      screen.getByText(/3 opérations de ce poste ne sont pas encore arrivées au poste principal/),
    ).toBeInTheDocument();
  });

  it("keeps M0's screen for a database_error: no move, the data folder to open", async () => {
    const calls = mockCommands({ app_open_folder: () => null });
    const user = userEvent.setup();
    renderWithProviders(<FatalScreen failure={tillFailure("database_error")} />);
    expect(screen.queryByRole("button", { name: rejoin })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: restore })).not.toBeInTheDocument();
    expect(screen.queryByText("Supérette El Amel")).not.toBeInTheDocument();
    const folder = screen.getByRole("button", { name: openFolder });
    expect(folder.className).toContain("bg-brand");
    await user.click(folder);
    expect(calls).toContain("app_open_folder");
    expect(calls).not.toContain("recovery_rejoin");
  });

  it("sends a till whose database is newer to the latest version, never to a move", () => {
    mockCommands({});
    renderWithProviders(<FatalScreen failure={tillFailure("database_newer_than_app")} />);
    expect(screen.queryByRole("button", { name: rejoin })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Télécharger la dernière version" })).toBeInTheDocument();
  });
});
