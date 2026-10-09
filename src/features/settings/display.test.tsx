import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { DisplaySettings } from "@/features/settings/DisplaySettings";
import { mockCommands, renderWithProviders } from "@/test/render";

describe("Settings › Display", () => {
  it("applies light mode and the text size to the whole window at once", async () => {
    let prefs = { lightMode: false, textSize: "normal", onscreenKeyboard: false };
    const saved: unknown[] = [];
    mockCommands({
      display_get: () => prefs,
      display_set: ({ prefs: next }: Record<string, unknown>) => {
        saved.push(next);
        prefs = next as typeof prefs;
        return prefs;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<DisplaySettings />);
    await user.click(await screen.findByRole("switch", { name: "Mode allégé (PC lent)" }));
    await waitFor(() => expect(document.documentElement.dataset.light).toBe("1"));
    await user.click(screen.getByRole("radio", { name: /Grande/ }));
    await waitFor(() => expect(document.documentElement.style.fontSize).toBe("115%"));
    expect(saved).toHaveLength(2);
  });
});
