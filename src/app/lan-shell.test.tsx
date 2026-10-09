import { QueryClientProvider } from "@tanstack/react-query";
import { emit } from "@tauri-apps/api/event";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, it, vi } from "vitest";
import { HostCloseGuard } from "@/app/HostCloseGuard";
import { LanBanner } from "@/app/LanBanner";
import { LanWatch } from "@/app/LanWatch";
import { SyncPill } from "@/app/SyncPill";
import { createQueryClient } from "@/lib/query";
import { ownerSession } from "@/test/fixtures";
import { clientStatusFixture, lanHostStatusFixture, lanTillStatusFixture } from "@/test/fixtures-m5";
import { mockCommands, renderWithProviders } from "@/test/render";

const offline = {
  ...lanTillStatusFixture,
  client: { ...clientStatusFixture, link: "offline" as const, pending: 3 },
};

describe("the LAN in the shell", () => {
  it("a till offline shows the pill and the banner with what is left to send", async () => {
    mockCommands({ lan_status: () => offline });
    renderWithProviders(
      <>
        <LanBanner />
        <SyncPill session={ownerSession} />
      </>,
    );
    expect(
      await screen.findByText("La vente continue hors ligne. 3 opérations à envoyer au poste principal."),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Hors ligne · 3 opérations à envoyer" })).toBeInTheDocument();
  });

  it("an online till shows no banner", async () => {
    mockCommands({ lan_status: () => lanTillStatusFixture });
    renderWithProviders(
      <>
        <LanBanner />
        <SyncPill session={ownerSession} />
      </>,
    );
    expect(await screen.findByRole("link", { name: "Connecté" })).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("warns when the till's clock is off by more than 5 minutes", async () => {
    mockCommands({
      lan_status: () => ({ ...lanTillStatusFixture, client: { ...clientStatusFixture, skewMs: 7_200_000 } }),
    });
    renderWithProviders(<LanBanner />);
    expect(
      await screen.findByText(
        "L'heure de ce PC diffère de 120 min de celle du poste principal. Corrigez l'heure de Windows.",
      ),
    ).toBeInTheDocument();
  });

  it("says why a till cannot sync at all instead of a refused operation", async () => {
    mockCommands({
      lan_status: () => ({
        ...lanTillStatusFixture,
        client: {
          ...clientStatusFixture,
          link: "attention" as const,
          rejected: 0,
          lastError: "lan_token_invalid",
        },
      }),
    });
    renderWithProviders(<LanBanner />);
    const banner = await screen.findByRole("status");
    expect(banner).not.toHaveTextContent("Le poste principal a refusé une opération");
    expect(banner.textContent).not.toBe("");
    expect(banner.textContent).not.toContain("lan_token_invalid");
  });

  it("keeps the refused-operation text when an operation was refused", async () => {
    mockCommands({
      lan_status: () => ({
        ...lanTillStatusFixture,
        client: {
          ...clientStatusFixture,
          link: "attention" as const,
          rejected: 1,
          lastError: "replay_invalid",
        },
      }),
    });
    renderWithProviders(<LanBanner />);
    expect(
      await screen.findByText(
        "Le poste principal a refusé une opération. La vente continue ; prévenez le propriétaire.",
      ),
    ).toBeInTheDocument();
  });

  it("the main PC's pill follows its tills without a pushed event", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      let online = 1;
      mockCommands({
        lan_status: () => ({
          ...lanHostStatusFixture,
          host: lanHostStatusFixture.host ? { ...lanHostStatusFixture.host, tillsOnline: online } : null,
        }),
      });
      renderWithProviders(<SyncPill session={ownerSession} />);
      expect(
        await screen.findByRole("link", { name: "Postes de caisse connectés : 1 / 2" }),
      ).toBeInTheDocument();
      online = 0;
      await act(() => vi.advanceTimersByTimeAsync(10_500));
      expect(
        await screen.findByRole("link", { name: "Postes de caisse connectés : 0 / 2" }),
      ).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("counts the tills on the main PC", async () => {
    mockCommands({ lan_status: () => lanHostStatusFixture });
    renderWithProviders(<SyncPill session={ownerSession} />);
    const pill = await screen.findByRole("link", { name: "Postes de caisse connectés : 1 / 2" });
    expect(pill).toHaveAttribute("href", "/settings/network");
  });

  it("refreshes the host data when the till applies a page", async () => {
    mockCommands({ lan_status: () => lanTillStatusFixture });
    const client = createQueryClient();
    client.setQueryData(["products", "grid", null, 200], []);
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <LanWatch />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    await waitFor(() => expect(client.getQueryData(["lan", "status"])).toBeDefined());
    expect(client.getQueryState(["products", "grid", null, 200])?.isInvalidated).toBe(false);
    await act(() =>
      emit("lan-status-changed", {
        status: { ...lanTillStatusFixture, client: { ...clientStatusFixture, appliedSeq: 121 } },
      }),
    );
    await waitFor(
      () => expect(client.getQueryState(["products", "grid", null, 200])?.isInvalidated).toBe(true),
      {
        timeout: 2_000,
      },
    );
  });

  it("asks before closing the main PC and closes on confirmation", async () => {
    const calls = mockCommands({ lan_confirm_close: () => null });
    const user = userEvent.setup();
    renderWithProviders(<HostCloseGuard />);
    // the listener is registered asynchronously: let it attach before the event
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
    await act(() => emit("lan-close-requested", { tillsOnline: 2 }));
    const dialog = await screen.findByRole("dialog", { name: "Fermer le poste principal ?" });
    await user.click(within(dialog).getByRole("button", { name: "Fermer" }));
    await waitFor(() => expect(calls).toContain("lan_confirm_close"));
  });
});
