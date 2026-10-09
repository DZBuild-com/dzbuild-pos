import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { LanBanner } from "@/app/LanBanner";
import { PendingEntriesCard } from "@/features/network/PendingEntriesCard";
import type { PendingEntries } from "@/lib/ipc/types";
import { clientStatusFixture } from "@/test/fixtures-m5";
import { mockCommands, renderWithProviders } from "@/test/render";

const waiting: PendingEntries = {
  entries: [
    {
      documentId: "0190a000-0000-7000-8000-0000000000d1",
      number: "C2-2026-000041",
      businessDate: "2026-10-04",
      kinds: ["ledgers"],
      refused: "voucher_balance_insufficient",
    },
    {
      documentId: "0190a000-0000-7000-8000-0000000000d2",
      number: "C2-2026-000042",
      businessDate: "2026-10-04",
      kinds: ["controlled", "serials"],
      refused: "value_invalid",
    },
  ],
  calls: [{ service: "deposits.take", at: 1_790_000_000_000, answered: false, refused: null }],
  refused: 2,
  refusedRegisters: 1,
};

describe("The till's entries waiting for the main PC", () => {
  it("lists each ticket with what waits and the refusal, warns about a legal register, and retries", async () => {
    const calls = mockCommands({
      lan_pending_entries: () => waiting,
      lan_pending_retry: () => ({ ...waiting, entries: [], calls: [], refused: 0, refusedRegisters: 0 }),
      lan_status: () => null,
    });
    const user = userEvent.setup();
    renderWithProviders(<PendingEntriesCard />);
    expect(await screen.findByText("Écritures en attente du poste principal")).toBeInTheDocument();
    expect(screen.getByText("3 en attente")).toBeInTheDocument();
    expect(screen.getByText("Ticket C2-2026-000041 du 04/10/2026")).toBeInTheDocument();
    expect(screen.getByText("Recharges, arrhes ou bons")).toBeInTheDocument();
    expect(
      screen.getByText("Registre des médicaments contrôlés · IMEI et numéros de série"),
    ).toBeInTheDocument();
    expect(screen.getByText("1 vente au registre légal refusée")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("le registre doit être complet");
    expect(screen.getByText(/Arrhes reçues/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Réessayer" }));
    await waitFor(() => expect(calls).toContain("lan_pending_retry"));
    expect(await screen.findByText("Rien en attente")).toBeInTheDocument();
  });

  it("shows every user a strip while the main PC refuses some of this till's entries", async () => {
    mockCommands({
      lan_status: () => ({
        role: "client",
        hostId: null,
        host: null,
        client: { ...clientStatusFixture, link: "online", entriesRefused: 2 },
      }),
    });
    renderWithProviders(<LanBanner />);
    expect(
      await screen.findByText(
        "Le poste principal a refusé 2 écritures de ce poste : voyez Réglages › Réseau.",
      ),
    ).toBeInTheDocument();
  });

  it("warns when the till's regulatory pack differs from the main PC's (final review)", async () => {
    mockCommands({
      lan_status: () => ({
        role: "client",
        hostId: null,
        host: null,
        client: { ...clientStatusFixture, link: "online", datapackGap: "behind" },
      }),
    });
    renderWithProviders(<LanBanner />);
    expect(
      await screen.findByText(
        "Le poste principal applique des règles fiscales plus récentes que ce poste : installez le même pack ici (Réglages › Données réglementaires).",
      ),
    ).toBeInTheDocument();
  });
});
