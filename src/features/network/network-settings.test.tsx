import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { NetworkSettings } from "@/features/network/NetworkSettings";
import { cashierSession, ownerSession } from "@/test/fixtures";
import {
  lanHostStatusFixture,
  lanStandaloneFixture,
  noticesFixture,
  tillsFixture,
  tillZFixture,
} from "@/test/fixtures-m5";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";

const firewallOk = {
  status: {
    tcpRule: "present",
    mdnsRule: "present",
    networks: [{ alias: "Ethernet", category: "private" }],
    ok: true,
  },
  commands: [
    'netsh advfirewall firewall add rule name="DZBuild POS LAN" dir=in action=allow protocol=TCP localport=7345 profile=private program="C:\\Program Files\\DZBuild POS\\DZBuild POS.exe" enable=yes',
    'netsh advfirewall firewall add rule name="DZBuild POS mDNS" dir=in action=allow protocol=UDP localport=5353 profile=private program="C:\\Program Files\\DZBuild POS\\DZBuild POS.exe" enable=yes',
  ],
};

function hostHandlers(extra: Record<string, Handler> = {}): Record<string, Handler> {
  return {
    lan_status: () => lanHostStatusFixture,
    lan_tills: () => tillsFixture,
    lan_notices: () => noticesFixture,
    lan_firewall: () => firewallOk,
    ...extra,
  };
}

describe("Settings › Réseau", () => {
  it("makes a single PC the main PC once the owner confirms", async () => {
    let status: unknown = lanStandaloneFixture;
    const calls = mockCommands(
      hostHandlers({
        lan_status: () => status,
        lan_become_host: () => {
          status = lanHostStatusFixture;
          return lanHostStatusFixture;
        },
      }),
    );
    const user = userEvent.setup();
    renderWithProviders(<NetworkSettings session={ownerSession} />);
    expect(await screen.findByText("Poste unique")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Faire de ce PC le poste principal" }));
    const dialog = await screen.findByRole("dialog", { name: "Faire de ce PC le poste principal ?" });
    await user.click(within(dialog).getByRole("button", { name: "Devenir le poste principal" }));
    await waitFor(() => expect(calls).toContain("lan_become_host"));
    expect(
      await screen.findByText("Prêt pour les postes de caisse à l'adresse 192.168.1.10:7345"),
    ).toBeInTheDocument();
    expect(screen.getByText("Postes de caisse connectés : 1 / 2")).toBeInTheDocument();
  });

  it("keeps the role for the owner", async () => {
    mockCommands({ lan_status: () => lanStandaloneFixture });
    renderWithProviders(<NetworkSettings session={{ ...cashierSession, permissions: ["settings.data"] }} />);
    expect(await screen.findByText("Seul le propriétaire change le rôle de ce PC.")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Faire de ce PC le poste principal" }),
    ).not.toBeInTheDocument();
  });

  it("does not go back to a single PC while tills are paired", async () => {
    mockCommands(hostHandlers());
    renderWithProviders(<NetworkSettings session={ownerSession} />);
    expect(await screen.findByRole("button", { name: "Revenir en poste unique" })).toBeDisabled();
    expect(screen.getByText("Retirez d'abord les postes de caisse.")).toBeInTheDocument();
  });

  it("shows a pairing code with its countdown and cancels it on close", async () => {
    const calls = mockCommands(
      hostHandlers({
        lan_pairing_code: () => ({ code: "042917", expiresAt: Date.now() + 600_000 }),
        lan_cancel_pairing: () => null,
      }),
    );
    const user = userEvent.setup();
    renderWithProviders(<NetworkSettings session={ownerSession} />);
    await user.click(await screen.findByRole("button", { name: "Ajouter un poste de caisse" }));
    const dialog = await screen.findByRole("dialog", { name: "Ajouter un poste de caisse" });
    expect(await within(dialog).findByText("042 917")).toBeInTheDocument();
    expect(within(dialog).getByText(/Valable encore (10:00|9:5\d)/)).toBeInTheDocument();
    expect(within(dialog).getByText("192.168.1.10:7345")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Fermer" }));
    await waitFor(() => expect(calls).toContain("lan_cancel_pairing"));
  });

  it("lists the tills with what needs checking, shows a till's Z and removes a till", async () => {
    const revoked: unknown[] = [];
    mockCommands(
      hostHandlers({
        lan_till_closures: () => tillZFixture,
        lan_revoke_till: ({ terminalId }) => {
          revoked.push(terminalId);
          return null;
        },
      }),
    );
    const user = userEvent.setup();
    renderWithProviders(<NetworkSettings session={ownerSession} />);
    const row = (await screen.findByText("Caisse 3")).closest("tr");
    if (!row) throw new Error("no row for Caisse 3");
    const cells = within(row);
    expect(cells.getByText("Hors ligne")).toBeInTheDocument();
    expect(cells.getByText("4 opérations à envoyer")).toBeInTheDocument();
    expect(cells.getByText("120 min d'écart")).toBeInTheDocument();
    expect(cells.getByText("À vérifier")).toBeInTheDocument();
    await user.click(cells.getByRole("button", { name: "Voir les Z" }));
    const sheet = await screen.findByRole("dialog", { name: "Z du poste C3" });
    expect(await within(sheet).findByText("Z2026-C3-000012")).toBeInTheDocument();
    expect(within(sheet).getByText("Z hors ligne")).toBeInTheDocument();
    await user.click(within(sheet).getByRole("button", { name: "Fermer" }));
    await user.click(cells.getByRole("button", { name: "Retirer" }));
    const confirm = await screen.findByRole("dialog", { name: "Retirer le poste Caisse 3 ?" });
    await user.click(within(confirm).getByRole("button", { name: "Retirer le poste" }));
    await waitFor(() => expect(revoked).toEqual([tillsFixture[1]?.terminalId]));
  });

  it("flags a till that holds the changes back, and warns before removing one with operations to send", async () => {
    mockCommands(
      hostHandlers({
        lan_tills: () => [tillsFixture[0], { ...tillsFixture[1]!, pinsFeed: true }],
      }),
    );
    const user = userEvent.setup();
    renderWithProviders(<NetworkSettings session={ownerSession} />);
    const row = (await screen.findByText("Caisse 3")).closest("tr");
    if (!row) throw new Error("no row for Caisse 3");
    expect(within(row).getByText("Silencieux : les modifications s'accumulent pour lui")).toBeInTheDocument();
    await user.click(within(row).getByRole("button", { name: "Retirer" }));
    const confirm = await screen.findByRole("dialog", { name: "Retirer le poste Caisse 3 ?" });
    expect(
      within(confirm).getByText(
        "Opérations encore sur ce poste : 4. Pour les livrer, il devra être appairé à nouveau, avec un nouveau code.",
      ),
    ).toBeInTheDocument();
  });

  it("marks a notice as read", async () => {
    const acknowledged: unknown[] = [];
    mockCommands(
      hostHandlers({
        lan_acknowledge_notice: ({ id }) => {
          acknowledged.push(id);
          return null;
        },
      }),
    );
    const user = userEvent.setup();
    renderWithProviders(<NetworkSettings session={ownerSession} />);
    const item = (await screen.findByText("Vendu sans stock (poste hors ligne)")).closest("li");
    if (!item) throw new Error("no notice");
    expect(within(item).getByText("Poste C3")).toBeInTheDocument();
    expect(within(item).getByText("Lait Candia 1 L")).toBeInTheDocument();
    await user.click(within(item).getByRole("button", { name: "Marquer comme lue" }));
    await waitFor(() => expect(acknowledged).toEqual([noticesFixture[0]?.id]));
  });

  it("explains a blocked firewall and gives the administrator's commands", async () => {
    mockCommands(
      hostHandlers({
        lan_firewall: () => ({
          ...firewallOk,
          status: {
            tcpRule: "missing",
            mdnsRule: "present",
            networks: [{ alias: "Wi-Fi", category: "public" }],
            ok: false,
          },
        }),
      }),
    );
    renderWithProviders(<NetworkSettings session={ownerSession} />);
    expect(
      await screen.findByText(
        "La règle du pare-feu manque : les postes de caisse ne pourront pas se connecter.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Windows classe le réseau « Wi-Fi » comme public : passez-le en réseau privé dans les paramètres de Windows.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText(firewallOk.commands[0] ?? "")).toBeInTheDocument();
  });

  it("says which firewall rule is missing and gives only its command", async () => {
    mockCommands(
      hostHandlers({
        lan_firewall: () => ({
          ...firewallOk,
          status: { ...firewallOk.status, mdnsRule: "disabled", ok: false },
        }),
      }),
    );
    renderWithProviders(<NetworkSettings session={ownerSession} />);
    const alert = (
      await screen.findByText(
        "La règle du pare-feu qui permet aux postes de caisse de trouver ce PC tout seuls (mDNS) manque. Tapez plutôt l'adresse de ce PC sur chaque poste de caisse.",
      )
    ).closest("[role=alert]");
    if (!(alert instanceof HTMLElement)) throw new Error("no firewall alert");
    expect(within(alert).getByText("Recherche de ce PC sur le réseau — mDNS (UDP 5353)")).toBeInTheDocument();
    expect(
      screen.queryByText("La règle du pare-feu manque : les postes de caisse ne pourront pas se connecter."),
    ).not.toBeInTheDocument();
    expect(screen.getByText(firewallOk.commands[1] ?? "")).toBeInTheDocument();
    expect(screen.queryByText(firewallOk.commands[0] ?? "")).not.toBeInTheDocument();
  });
});
