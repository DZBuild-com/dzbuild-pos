import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { FatalScreen } from "@/app/FatalScreen";
import { missingKeys, mockCommands, renderWithProviders } from "@/test/render";

const backups = [
  {
    id: "r0",
    fileName: "pos-20260928-083000-000-auto.db",
    at: 1_790_577_000_000,
    size: 5_242_880,
    kind: "auto",
    location: "local",
    place: null,
  },
  {
    id: "r1",
    fileName: "pos-20260927-200000-000-auto.db",
    at: 1_790_535_600_000,
    size: 5_200_000,
    kind: "auto",
    location: "drive",
    place: "E:\\",
  },
];

const valid = {
  status: "valid",
  schemaVersion: 3,
  appSchemaVersion: 3,
  companyName: "Supérette El Amel",
  lastTicket: { number: "TK2026-C1-000148", at: 1_790_576_900_000 },
  documents: 148,
  products: 512,
  customers: 37,
  lastActivityAt: 1_790_577_000_000,
  owners: [{ id: "u1", displayName: "Amine" }],
};

const failure = (code: string) => ({
  error: { code },
  dataDir: "C:\\Users\\amine\\AppData\\Roaming\\dev.dzbuild.pos",
});

describe("guided restore from the fatal screen", () => {
  it("a lost database: the owner may create a new, empty shop anyway with the backup's PIN (finding 10)", async () => {
    const fresh: unknown[] = [];
    mockCommands({
      recovery_list: () => backups,
      recovery_inspect: () => valid,
      recovery_start_fresh: ({ input }: Record<string, unknown>) => {
        fresh.push(input);
        return null;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<FatalScreen failure={failure("database_missing")} />);
    await user.click(screen.getByRole("button", { name: "Restaurer une sauvegarde" }));
    await screen.findByRole("radiogroup", { name: "Sauvegardes trouvées" });
    await user.click(screen.getByRole("button", { name: "Vérifier cette sauvegarde" }));
    await user.click(await screen.findByRole("button", { name: "Continuer" }));
    const anyway = screen.getByRole("button", { name: "Créer une nouvelle boutique quand même" });
    expect(anyway).toBeDisabled();
    await user.type(screen.getByLabelText("Code PIN du propriétaire (celui de cette sauvegarde)"), "2580");
    await user.click(anyway);
    await user.click(screen.getByRole("button", { name: "Oui, créer une boutique vide et redémarrer" }));
    await waitFor(() => expect(fresh).toEqual([{ id: "r0", ownerId: "u1", pin: "2580" }]));
    expect(await screen.findByText("Redémarrage sur une nouvelle boutique…")).toBeInTheDocument();
  });

  it("never offers an empty shop for a damaged database, and names a demo or till copy", async () => {
    mockCommands({
      recovery_list: () => backups,
      recovery_inspect: () => ({
        ...valid,
        origin: { demo: true, lanRole: "client", hostName: "Principal", terminalCode: "C2", activeTills: 0 },
      }),
    });
    const user = userEvent.setup();
    renderWithProviders(<FatalScreen failure={failure("database_corrupt")} />);
    await user.click(screen.getByRole("button", { name: "Restaurer une sauvegarde" }));
    await screen.findByRole("radiogroup", { name: "Sauvegardes trouvées" });
    await user.click(screen.getByRole("button", { name: "Vérifier cette sauvegarde" }));
    expect(await screen.findByText("DÉMO – boutique de démonstration")).toBeInTheDocument();
    expect(screen.getByText(/le poste de caisse C2 de « Principal »/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Continuer" }));
    expect(
      screen.queryByRole("button", { name: "Créer une nouvelle boutique quand même" }),
    ).not.toBeInTheDocument();
  });

  it("explains a main PC with tills before any backup is listed", () => {
    mockCommands({});
    renderWithProviders(<FatalScreen failure={{ ...failure("database_corrupt"), mainPcTills: 2 }} />);
    expect(screen.getByText(/Ce PC est le poste principal de 2 postes de caisse/)).toBeInTheDocument();
  });

  it("lists the backups, checks the owner's PIN against the backup and restarts", async () => {
    const restores: unknown[] = [];
    mockCommands({
      recovery_list: () => backups,
      recovery_inspect: () => valid,
      recovery_restore: ({ input }: Record<string, unknown>) => {
        restores.push(input);
        return {
          damagedFolder: "C:\\data\\backups\\corrupt-20260928-091500",
          moved: ["pos.db", "pos.db-wal"],
          restoredFrom: "C:\\data\\backups\\x.db",
        };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<FatalScreen failure={failure("database_corrupt")} />);
    await user.click(screen.getByRole("button", { name: "Restaurer une sauvegarde" }));
    const list = await screen.findByRole("radiogroup", { name: "Sauvegardes trouvées" });
    expect(within(list).getAllByRole("radio")).toHaveLength(2);
    expect(within(list).getByText(/Clé USB/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Vérifier cette sauvegarde" }));
    expect(await screen.findByText("TK2026-C1-000148")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Continuer" }));
    expect(screen.getByText(/jamais effacés/)).toBeInTheDocument();
    const go = screen.getByRole("button", { name: "Restaurer et redémarrer" });
    expect(go).toBeDisabled();
    await user.type(screen.getByLabelText("Code PIN du propriétaire (celui de cette sauvegarde)"), "2580");
    await user.click(go);
    await waitFor(() => expect(restores).toEqual([{ id: "r0", ownerId: "u1", pin: "2580" }]));
    expect(await screen.findByRole("status")).toHaveTextContent(/Redémarrage/);
    expect(screen.getByText("C:\\data\\backups\\corrupt-20260928-091500")).toBeInTheDocument();
  });

  it("a newer backup explains why and offers no restore button", async () => {
    mockCommands({
      recovery_list: () => backups,
      recovery_inspect: () => ({ ...valid, status: "newer_than_app", schemaVersion: 9, owners: [] }),
    });
    const user = userEvent.setup();
    renderWithProviders(<FatalScreen failure={failure("database_missing")} />);
    await user.click(screen.getByRole("button", { name: "Restaurer une sauvegarde" }));
    await user.click(await screen.findByRole("button", { name: "Vérifier cette sauvegarde" }));
    expect(await screen.findByText(/version plus récente de DZBuild POS/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Continuer" })).not.toBeInTheDocument();
  });

  it("a database from a newer version sends the owner to the download page", async () => {
    const calls = mockCommands({ app_open_link: () => null });
    const user = userEvent.setup();
    renderWithProviders(<FatalScreen failure={failure("database_newer_than_app")} />);
    expect(screen.queryByRole("button", { name: "Restaurer une sauvegarde" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Télécharger la dernière version" }));
    expect(calls).toContain("app_open_link");
  });

  it("speaks Arabic right to left", async () => {
    mockCommands({ recovery_list: () => backups });
    const { setLanguage } = await import("@/lib/i18n");
    await setLanguage("ar");
    const user = userEvent.setup();
    renderWithProviders(<FatalScreen failure={failure("database_corrupt")} />);
    await user.click(screen.getByRole("button", { name: "استرجاع نسخة احتياطية" }));
    expect(await screen.findByRole("radiogroup", { name: "النسخ الاحتياطية الموجودة" })).toBeInTheDocument();
    expect(document.documentElement.dir).toBe("rtl");
  });

  it("moves through the backups with the arrow keys and checks the one chosen", async () => {
    const inspected: unknown[] = [];
    mockCommands({
      recovery_list: () => backups,
      recovery_inspect: ({ id }: Record<string, unknown>) => {
        inspected.push(id);
        return valid;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<FatalScreen failure={failure("database_corrupt")} />);
    await user.click(screen.getByRole("button", { name: "Restaurer une sauvegarde" }));
    const list = await screen.findByRole("radiogroup", { name: "Sauvegardes trouvées" });
    const [newest, older] = within(list).getAllByRole("radio");
    expect(newest).toHaveAttribute("aria-checked", "true");
    newest?.focus();
    await user.keyboard("{ArrowDown}");
    expect(older).toHaveAttribute("aria-checked", "true");
    expect(older).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Vérifier cette sauvegarde" }));
    await waitFor(() => expect(inspected).toEqual(["r1"]));
  });

  it("says the PIN is locked for 15 minutes after too many wrong PINs", async () => {
    mockCommands({
      recovery_list: () => backups,
      recovery_inspect: () => valid,
      recovery_restore: () => Promise.reject({ code: "too_many_attempts" }),
    });
    const user = userEvent.setup();
    renderWithProviders(<FatalScreen failure={failure("database_corrupt")} />);
    await user.click(screen.getByRole("button", { name: "Restaurer une sauvegarde" }));
    await user.click(await screen.findByRole("button", { name: "Vérifier cette sauvegarde" }));
    await user.click(await screen.findByRole("button", { name: "Continuer" }));
    await user.type(screen.getByLabelText("Code PIN du propriétaire (celui de cette sauvegarde)"), "1111");
    await user.click(screen.getByRole("button", { name: "Restaurer et redémarrer" }));
    expect(await screen.findByText(/15 minutes/)).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("an empty list asks for the USB stick", async () => {
    mockCommands({ recovery_list: () => [] });
    const user = userEvent.setup();
    renderWithProviders(<FatalScreen failure={failure("database_corrupt")} />);
    await user.click(screen.getByRole("button", { name: "Restaurer une sauvegarde" }));
    expect(await screen.findByText(/Branchez la clé USB/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Vérifier cette sauvegarde" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Annuler" }));
    expect(screen.getByRole("button", { name: "Restaurer une sauvegarde" })).toBeInTheDocument();
  });
});

describe("a till's fatal screen", () => {
  const till = { ...failure("database_corrupt"), tillOf: "Supérette El Amel" };

  it("joins the main PC again instead of restoring a backup", async () => {
    const calls = mockCommands({
      recovery_rejoin: () => ({
        damagedFolder: "C:\\data\\backups\\corrupt-20260928-091500",
        moved: ["pos.db"],
      }),
    });
    const user = userEvent.setup();
    renderWithProviders(<FatalScreen failure={till} />);
    expect(screen.queryByRole("button", { name: "Restaurer une sauvegarde" })).not.toBeInTheDocument();
    expect(screen.getByText("Supérette El Amel")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Rejoindre de nouveau le poste principal" }));
    expect(screen.getByText(/jamais effacés/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Déplacer les fichiers et redémarrer" }));
    expect(await screen.findByRole("status")).toHaveTextContent(/nouveau PC/);
    expect(screen.getByText("C:\\data\\backups\\corrupt-20260928-091500")).toBeInTheDocument();
    expect(calls).toContain("recovery_rejoin");
    expect(calls).not.toContain("recovery_list");
  });

  it("has every text of both screens in Arabic and English", async () => {
    mockCommands({ recovery_list: () => backups, recovery_inspect: () => valid });
    const { i18n, setLanguage } = await import("@/lib/i18n");
    const button = (key: string) => screen.getByRole("button", { name: i18n.t(key) });
    for (const lang of ["ar", "en"] as const) {
      await setLanguage(lang);
      missingKeys.clear();
      const user = userEvent.setup();
      const tillScreen = renderWithProviders(<FatalScreen failure={till} />);
      await user.click(button("common:recovery.till.start"));
      expect(button("common:recovery.till.confirm")).toBeEnabled();
      tillScreen.unmount();
      const restoreScreen = renderWithProviders(<FatalScreen failure={failure("database_corrupt")} />);
      await user.click(button("common:recovery.start"));
      await screen.findByRole("radiogroup", { name: i18n.t("common:recovery.found") });
      await user.click(button("common:recovery.check"));
      await screen.findByText("TK2026-C1-000148");
      await user.click(button("common:recovery.continue"));
      expect(button("common:recovery.restore")).toBeDisabled();
      expect([...missingKeys]).toEqual([]);
      restoreScreen.unmount();
    }
  });
});
