import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { JoinWizard } from "@/features/network/JoinWizard";
import { TillPanel } from "@/features/network/TillPanel";
import { hostAddress, validateJoin } from "@/features/network/join-form";
import { appInfoFixture, cashierSession, ownerSession } from "@/test/fixtures";
import { clientStatusFixture } from "@/test/fixtures-m5";
import { mockCommands, renderWithProviders } from "@/test/render";

const found = {
  hostId: "0190a000-0000-7000-8000-0000000000a1",
  name: "Supérette El Amel",
  addresses: ["fe80::1", "192.168.1.10"],
  port: 7345,
  appVersion: appInfoFixture.version,
  schemaVersion: appInfoFixture.schemaVersion,
};

const health = {
  hostId: found.hostId,
  shopName: "Supérette El Amel",
  appVersion: appInfoFixture.version,
  schemaVersion: appInfoFixture.schemaVersion,
  hostTime: 1_790_000_000_000,
  pairingOpen: true,
  suggestedCode: "C2",
};

describe("join form", () => {
  it("checks the code, the name and the series like the main PC does", () => {
    expect(validateJoin({ code: "12345", terminalName: " ", terminalCode: "2C" })).toEqual({
      code: "sync:lan.join.codeInvalid",
      terminalName: "validation:required",
      terminalCode: "sync:lan.join.seriesInvalid",
    });
    expect(validateJoin({ code: "042917", terminalName: "Caisse 2", terminalCode: "C2" })).toEqual({});
    expect(hostAddress(found)).toBe("192.168.1.10:7345");
    expect(hostAddress({ ...found, addresses: [] })).toBeNull();
  });
});

describe("JoinWizard", () => {
  it("finds the main PC, joins with the code and restarts", async () => {
    const joined: unknown[] = [];
    const calls = mockCommands({
      app_info: () => appInfoFixture,
      lan_discover: () => [found],
      lan_probe: () => health,
      lan_join: ({ input }) => {
        joined.push(input);
        return { hostName: "Supérette El Amel", terminalCode: "C2" };
      },
      lan_restart: () => null,
    });
    const user = userEvent.setup();
    renderWithProviders(<JoinWizard onBack={() => undefined} />);
    expect(await screen.findByText("Supérette El Amel")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Choisir" }));
    const code = await screen.findByLabelText("Code d'appairage (6 chiffres)");
    expect(screen.getByLabelText("Nom de ce poste")).toHaveValue("Caisse C2");
    expect(screen.getByLabelText("Série du poste")).toHaveValue("C2");
    await user.type(code, "04291");
    await user.click(screen.getByRole("button", { name: "Rejoindre" }));
    expect(await screen.findByText("Le code d'appairage a 6 chiffres.")).toBeInTheDocument();
    expect(joined).toHaveLength(0);
    await user.type(code, "7");
    await user.click(screen.getByRole("button", { name: "Rejoindre" }));
    await waitFor(() => expect(joined).toHaveLength(1));
    expect(joined[0]).toEqual({
      address: "192.168.1.10:7345",
      code: "042917",
      terminalCode: "C2",
      terminalName: "Caisse C2",
    });
    expect(
      await screen.findByText(
        "Ce PC est prêt. Il redémarre pour devenir le poste de caisse C2 de Supérette El Amel.",
      ),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Redémarrer maintenant" }));
    await waitFor(() => expect(calls).toContain("lan_restart"));
  });

  it("types the address when nothing answers on the network", async () => {
    const probed: unknown[] = [];
    mockCommands({
      app_info: () => appInfoFixture,
      lan_discover: () => [],
      lan_probe: ({ address }) => {
        probed.push(address);
        return { ...health, pairingOpen: false };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<JoinWizard onBack={() => undefined} />);
    expect(await screen.findByText("Aucun poste principal trouvé. Tapez son adresse.")).toBeInTheDocument();
    await user.type(screen.getByLabelText("Adresse du poste principal"), "192.168.1.10");
    await user.click(screen.getByRole("button", { name: "Vérifier" }));
    await waitFor(() => expect(probed).toEqual(["192.168.1.10"]));
    expect(
      await screen.findByText(
        "Le poste principal n'affiche pas de code. Sur le poste principal : Réglages › Réseau › Ajouter un poste de caisse.",
      ),
    ).toBeInTheDocument();
  });

  it("refuses a main PC on another version", async () => {
    mockCommands({
      app_info: () => appInfoFixture,
      lan_discover: () => [found],
      lan_probe: () => ({ ...health, appVersion: "9.9.9" }),
    });
    const user = userEvent.setup();
    renderWithProviders(<JoinWizard onBack={() => undefined} />);
    await user.click(await screen.findByRole("button", { name: "Choisir" }));
    expect(
      await screen.findByText(
        "Le poste principal n'a pas la même version de DZBuild POS. Mettez les deux PC à la même version.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Rejoindre" })).toBeDisabled();
  });

  it("says why the main PC refused the code", async () => {
    mockCommands({
      app_info: () => appInfoFixture,
      lan_discover: () => [found],
      lan_probe: () => health,
      lan_join: () => Promise.reject({ code: "pairing_code_invalid" }),
    });
    const user = userEvent.setup();
    renderWithProviders(<JoinWizard onBack={() => undefined} />);
    await user.click(await screen.findByRole("button", { name: "Choisir" }));
    await user.type(await screen.findByLabelText("Code d'appairage (6 chiffres)"), "000000");
    await user.click(screen.getByRole("button", { name: "Rejoindre" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Ce code d'appairage n'est pas valable. Demandez un nouveau code sur le poste principal.",
    );
  });

  it("asks the owner's PIN on a PC already set up and sends it with the code", async () => {
    const joined: unknown[] = [];
    mockCommands({
      app_info: () => appInfoFixture,
      setup_status: () => ({ completed: true, hasOwner: true, terminalCode: "C1", loginLang: "fr" }),
      lan_discover: () => [found],
      lan_probe: () => health,
      lan_join: ({ input }) => {
        joined.push(input);
        return Promise.reject({ code: "invalid_pin" });
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<JoinWizard titled={false} onBack={() => undefined} />);
    expect(screen.queryByRole("heading", { name: "Rejoindre le poste principal" })).toBeNull();
    await user.click(await screen.findByRole("button", { name: "Choisir" }));
    await user.type(await screen.findByLabelText("Code d'appairage (6 chiffres)"), "042917");
    const pin = await screen.findByLabelText("Code PIN du propriétaire");
    await user.click(screen.getByRole("button", { name: "Rejoindre" }));
    expect(await screen.findByText("Remplissez ce champ.")).toBeInTheDocument();
    expect(joined).toHaveLength(0);
    await user.type(pin, "2580");
    await user.click(screen.getByRole("button", { name: "Rejoindre" }));
    await waitFor(() => expect(joined).toHaveLength(1));
    expect(joined[0]).toEqual({
      address: "192.168.1.10:7345",
      code: "042917",
      terminalCode: "C2",
      terminalName: "Caisse C2",
      ownerPin: "2580",
    });
    expect(await screen.findByText("Code PIN erroné. Retapez-le.")).toBeInTheDocument();
  });
});

describe("JoinWizard refusals", () => {
  it("says at its first step that a PC linked to DZBuild unlinks first", async () => {
    mockCommands({
      lan_discover: () =>
        Promise.reject({
          code: "cloud_unlink_first",
          details: { store: "El Amel en ligne", pending: 3 },
        }),
      app_info: () => appInfoFixture,
      setup_status: () => ({ completed: true }),
    });
    renderWithProviders(<JoinWizard onBack={() => {}} />);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("El Amel en ligne");
    expect(alert).toHaveTextContent("Réglages › DZBuild");
    expect(screen.queryByText("Aucun poste principal trouvé. Tapez son adresse.")).not.toBeInTheDocument();
  });
});

describe("TillPanel", () => {
  it("retries the refused operation and sends now", async () => {
    const calls = mockCommands({
      lan_retry_rejected: () => null,
      lan_sync_now: () => null,
      lan_status: () => null,
    });
    const user = userEvent.setup();
    renderWithProviders(
      <TillPanel
        session={ownerSession}
        status={{ ...clientStatusFixture, link: "attention", rejected: 1, lastError: "replay_forbidden_row" }}
      />,
    );
    expect(screen.getByText("1 opération refusée par le poste principal")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Réessayer" }));
    await user.click(screen.getByRole("button", { name: "Envoyer et recevoir maintenant" }));
    await waitFor(() =>
      expect(calls).toEqual(expect.arrayContaining(["lan_retry_rejected", "lan_sync_now"])),
    );
  });

  it("makes this till the main PC only with the owner's PIN, and says when the old one still answers", async () => {
    const promoted: unknown[] = [];
    mockCommands({
      lan_promote: ({ pin }) => {
        promoted.push(pin);
        return Promise.reject({ code: "lan_host_alive" });
      },
    });
    const user = userEvent.setup();
    renderWithProviders(
      <TillPanel session={ownerSession} status={{ ...clientStatusFixture, link: "offline" }} />,
    );
    await user.click(screen.getByRole("button", { name: "Faire de ce poste le poste principal" }));
    const confirm = screen.getByRole("button", { name: "Devenir le poste principal" });
    expect(confirm).toBeDisabled();
    await user.type(screen.getByLabelText("Code PIN du propriétaire"), "2580");
    await user.click(confirm);
    await waitFor(() => expect(promoted).toEqual(["2580"]));
    expect(
      await screen.findByText(
        "Le poste principal répond encore. On ne fait d'un poste de caisse le poste principal que si l'ancien est hors service pour de bon.",
      ),
    ).toBeInTheDocument();
  });

  it("offers to join the main PC again when the till's token is refused, with what the owner needs", async () => {
    mockCommands({
      lan_discover: () => [],
      app_info: () => appInfoFixture,
      setup_status: () => ({ completed: true }),
    });
    const user = userEvent.setup();
    renderWithProviders(
      <TillPanel
        session={ownerSession}
        status={{ ...clientStatusFixture, link: "attention", rejected: 0, lastError: "lan_token_invalid" }}
      />,
    );
    expect(
      screen.getByText(
        "Demandez un nouveau code au propriétaire sur le poste principal : les opérations encore sur ce poste lui seront envoyées avant la nouvelle copie.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("1 opération refusée par le poste principal")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Rejoindre à nouveau le poste principal" }));
    expect(
      await screen.findByRole("dialog", { name: "Rejoindre à nouveau le poste principal" }),
    ).toBeInTheDocument();
  });

  it("tells the owner that DZBuild must be linked again after a promotion", async () => {
    mockCommands({});
    const user = userEvent.setup();
    renderWithProviders(
      <TillPanel session={ownerSession} status={{ ...clientStatusFixture, link: "offline" }} />,
    );
    await user.click(screen.getByRole("button", { name: "Faire de ce poste le poste principal" }));
    expect(
      screen.getByText(
        "La liaison DZBuild ne suit pas : liez à nouveau la boutique sur ce poste dans Réglages › DZBuild.",
      ),
    ).toBeInTheDocument();
  });

  it("hides the role changes from a cashier", () => {
    mockCommands({});
    renderWithProviders(<TillPanel session={cashierSession} status={clientStatusFixture} />);
    expect(screen.queryByRole("button", { name: "Quitter le réseau" })).not.toBeInTheDocument();
    expect(screen.getByText("Poste principal : Supérette El Amel (192.168.1.10:7345)")).toBeInTheDocument();
  });
});
