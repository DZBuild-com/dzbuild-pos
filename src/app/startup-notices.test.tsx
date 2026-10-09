import { fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { StartupNotices } from "@/app/StartupNotices";
import { mockCommands, renderWithProviders } from "@/test/render";

describe("startup notices of M6", () => {
  it("offers the WebView2 download, explains a clock set back and names the new version", async () => {
    const calls = mockCommands({
      app_startup_error: () => null,
      app_startup_notices: () => [
        { kind: "webviewOutdated", version: "119.0.2151.97", minimum: "120.0.2210.55" },
        { kind: "clockMovedBack", lastSeenAt: 1_790_593_200_000, now: 1_790_334_000_000 },
        { kind: "appUpdated", from: "0.6.0", to: "0.6.1" },
      ],
      app_open_link: () => null,
    });
    renderWithProviders(<StartupNotices />);
    expect(
      await screen.findByText("Le composant d'affichage de Windows (WebView2) est trop ancien"),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Mettre à jour WebView2" }));
    await waitFor(() => expect(calls).toContain("app_open_link"));
    expect(screen.getByText("L'horloge du PC a reculé")).toBeInTheDocument();
    expect(screen.getByText(/0\.6\.0.*0\.6\.1/)).toBeInTheDocument();
  });

  it("names a regulatory data pack the start left aside, with the reason (PLT-024)", async () => {
    mockCommands({
      app_startup_error: () => null,
      app_startup_notices: () => [
        { kind: "datapackSkipped", file: "dz-datapack-2027.json", code: "datapack_signature_invalid" },
      ],
    });
    renderWithProviders(<StartupNotices />);
    expect(await screen.findByText(/Le fichier réglementaire .*dz-datapack-2027\.json/)).toBeInTheDocument();
  });
});
