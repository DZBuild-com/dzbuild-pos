import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { LoginScreen } from "@/features/auth/LoginScreen";
import { ownerSession, ownerTile, settingsValues } from "@/test/fixtures";
import { activeStatus } from "@/test/fixtures-m9";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";

function mockLogin(login: Handler) {
  return mockCommands({
    auth_list_users: () => [ownerTile],
    // M9: the sign-in's licence banner (a lifetime licence: no banner)
    license_status: () => activeStatus,
    company_get: () => null,
    settings_get: ({ key }) => settingsValues[String(key)],
    settings_set: ({ value }) => value,
    auth_login: login,
  });
}

describe("LoginScreen", () => {
  it("logs in with the PIN typed on the keyboard", async () => {
    const received: unknown[] = [];
    mockLogin((args) => {
      received.push(args);
      return ownerSession;
    });
    const user = userEvent.setup();
    renderWithProviders(<LoginScreen />);
    await user.click(await screen.findByRole("button", { name: /Amine Benali/ }));
    // The tile unmounts; the pad focuses itself (Task 18), so typing needs no extra click.
    await waitFor(() => expect(screen.getByRole("group", { name: "Code PIN" })).toHaveFocus());
    await user.keyboard("[Digit2][Digit5][Digit8][Digit0]{Enter}");
    await waitFor(() => expect(received).toEqual([{ input: { user: "u-owner", pin: "2580" } }]));
  });

  it("shows the translated error and clears the PIN", async () => {
    mockLogin(() => Promise.reject({ code: "invalid_pin" }));
    const user = userEvent.setup();
    renderWithProviders(<LoginScreen />);
    await user.click(await screen.findByRole("button", { name: /Amine Benali/ }));
    await waitFor(() => expect(screen.getByRole("group", { name: "Code PIN" })).toHaveFocus());
    await user.keyboard("25800{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("Code PIN erroné. Retapez-le.");
    expect(screen.getByText("Chiffres saisis : 0")).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Code PIN" })).toHaveFocus();
  });

  it("switches the login screen to Arabic and remembers it", async () => {
    const calls = mockLogin(() => ownerSession);
    const user = userEvent.setup();
    renderWithProviders(<LoginScreen />);
    await user.click(await screen.findByRole("radio", { name: "العربية" }));
    // Arabic's texts load on first use (src/lib/i18n.ts).
    await waitFor(() => expect(document.documentElement).toHaveAttribute("dir", "rtl"));
    expect(await screen.findByText("من يعمل الآن؟")).toBeInTheDocument();
    await waitFor(() => expect(calls).toContain("settings_set"));
  });
});
