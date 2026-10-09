import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { StartupNotices } from "@/app/StartupNotices";
import { mockCommands, renderWithProviders } from "@/test/render";

describe("an update Windows did not start", () => {
  it("explains after the restart that the version was not installed and the data is intact", async () => {
    mockCommands({
      app_startup_error: () => null,
      app_startup_notices: () => [{ kind: "updateNotInstalled", version: "0.6.1" }],
    });
    renderWithProviders(<StartupNotices />);
    expect(await screen.findByText("La mise à jour n'a pas été installée")).toBeInTheDocument();
    expect(screen.getByText(/0\.6\.1.*vos données sont intactes/)).toBeInTheDocument();
  });
});
