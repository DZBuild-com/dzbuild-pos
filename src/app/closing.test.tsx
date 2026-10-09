import { emit } from "@tauri-apps/api/event";
import { act, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ClosingOverlay } from "@/app/ClosingOverlay";
import { mockCommands, renderWithProviders } from "@/test/render";

describe("closing overlay", () => {
  it("says the app is saving before it closes", async () => {
    mockCommands({});
    renderWithProviders(<ClosingOverlay />);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    await act(async () => {
      await emit("app-closing", { backup: true });
    });
    expect(await screen.findByText("Sauvegarde avant fermeture…")).toBeInTheDocument();
  });
});
