import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { AuditSettings } from "@/features/settings/AuditSettings";
import { BackupsSettings } from "@/features/settings/BackupsSettings";
import { SettingsLayout } from "@/features/settings/SettingsLayout";
import { ShopSettings } from "@/features/settings/ShopSettings";
import { UsersSettings } from "@/features/settings/UsersSettings";
import { setLanguage } from "@/lib/i18n";
import {
  auditRowFixture,
  backupFixture,
  CASHIER_ROLE_ID,
  cashierSession,
  catalogFixture,
  companyFixture,
  ownerSession,
  packFixture,
  rolesFixture,
  settingsValues,
  userFixture,
} from "@/test/fixtures";
import { lanTillStatusFixture } from "@/test/fixtures-m5";
import { sessionWith } from "@/test/fixtures-m8";
import { mockCommands, renderWithProviders } from "@/test/render";

const settingsHandlers = {
  settings_get: ({ key }: Record<string, unknown>) => settingsValues[String(key)],
  settings_set: ({ value }: Record<string, unknown>) => value,
};

describe("Settings › sections", () => {
  it("lists Réseau after Matériel for the owner, and hides it from a cashier", () => {
    const { unmount } = renderWithProviders(<SettingsLayout session={ownerSession} />);
    const owner = screen.getAllByRole("link").map((link) => link.textContent);
    expect(owner.indexOf("Réseau")).toBe(owner.indexOf("Matériel") + 1);
    unmount();
    renderWithProviders(<SettingsLayout session={cashierSession} />);
    expect(screen.queryByRole("link", { name: "Réseau" })).not.toBeInTheDocument();
  });

  it("lists Activité et modules for settings.company holders", () => {
    const { unmount } = renderWithProviders(<SettingsLayout session={sessionWith(["settings.company"])} />);
    const links = screen.getAllByRole("link").map((link) => link.textContent);
    expect(links.indexOf("Activité et modules")).toBe(links.indexOf("Boutique") + 1);
    unmount();
    renderWithProviders(<SettingsLayout session={cashierSession} />);
    expect(screen.queryByRole("link", { name: "Activité et modules" })).not.toBeInTheDocument();
  });
});

describe("Settings › Shop", () => {
  it("is read-only on a till, with the main PC's name and no save button", async () => {
    mockCommands({
      ...settingsHandlers,
      company_get: () => companyFixture,
      datapack_get: () => packFixture,
      lan_status: () => lanTillStatusFixture,
    });
    renderWithProviders(<ShopSettings />);
    expect(
      await screen.findByText(
        "Lecture seule sur ce poste de caisse : faites-le sur le poste principal (Supérette El Amel).",
      ),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("NIF (15 ou 18 chiffres)")).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Enregistrer la boutique" })).not.toBeInTheDocument();
  });

  it("validates the NIF, then saves the capital in centimes", async () => {
    const saved: unknown[] = [];
    mockCommands({
      ...settingsHandlers,
      company_get: () => companyFixture,
      datapack_get: () => packFixture,
      company_update: (args) => {
        saved.push(args);
        return companyFixture;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ShopSettings />);
    const nif = await screen.findByLabelText("NIF (15 ou 18 chiffres)");
    await user.type(nif, "12345");
    await user.click(screen.getByRole("button", { name: "Enregistrer la boutique" }));
    expect(
      await screen.findByText("Le NIF doit contenir 15 chiffres (18 pour un établissement secondaire)."),
    ).toBeInTheDocument();
    expect(saved).toHaveLength(0);
    await user.clear(nif);
    await user.type(nif, "0001 1600 1234 567");
    await user.type(screen.getByLabelText("Capital social (DA)"), "1 000 000,50");
    await user.click(screen.getByRole("button", { name: "Enregistrer la boutique" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({
      input: {
        nameFr: "Supérette El Amel",
        nif: "000116001234567",
        capital: 100_000_050,
        wilaya: "16",
        taxRegime: "ifu",
      },
    });
  });

  it("reads Arabic-Indic digits and checks the NIS like the NIF", async () => {
    const saved: unknown[] = [];
    mockCommands({
      ...settingsHandlers,
      company_get: () => companyFixture,
      datapack_get: () => packFixture,
      company_update: (args) => {
        saved.push(args);
        return companyFixture;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ShopSettings />);
    await user.type(await screen.findByLabelText("NIF (15 ou 18 chiffres)"), "٠٠٠١ ١٦٠٠ ١٢٣٤ ٥٦٧");
    const nis = screen.getByLabelText("NIS (15 ou 18 chiffres)");
    await user.type(nis, "١٢٣");
    await user.type(screen.getByLabelText("Téléphone"), "٠٥٥٥ ١٢ ٣٤ ٥٦");
    await user.click(screen.getByRole("button", { name: "Enregistrer la boutique" }));
    expect(await screen.findByText("Le NIS doit contenir 15 ou 18 chiffres.")).toBeInTheDocument();
    expect(saved).toHaveLength(0);
    await user.clear(nis);
    await user.type(nis, "٠٩٨١ ١٦٠١ ٢٣٤٥ ٦٧٨");
    await user.click(screen.getByRole("button", { name: "Enregistrer la boutique" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({
      input: { nif: "000116001234567", nis: "098116012345678", phone: "0555 12 34 56" },
    });
  });

  it("sends a PNG logo of at most 512 KB and refuses a bigger one", async () => {
    const logos: unknown[] = [];
    mockCommands({
      ...settingsHandlers,
      company_get: () => companyFixture,
      datapack_get: () => packFixture,
      company_set_logo: (args) => {
        logos.push(args);
        return { ...companyFixture, hasLogo: args.logo !== null, updatedAt: companyFixture.updatedAt + 1 };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ShopSettings />);
    const input = await screen.findByLabelText("Choisir une image");
    expect(screen.getByText("Aucun logo pour le moment.")).toBeInTheDocument();
    await user.upload(input, new File([new Uint8Array(512 * 1024 + 1)], "grand.png", { type: "image/png" }));
    expect(
      await screen.findByText("Le logo dépasse 512 Ko. Choisissez une image plus légère."),
    ).toBeInTheDocument();
    expect(logos).toHaveLength(0);
    const png = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00];
    await user.upload(input, new File([new Uint8Array(png)], "logo.png", { type: "image/png" }));
    await waitFor(() => expect(logos).toEqual([{ logo: png }]));
    expect(await screen.findByText("Un logo est enregistré.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retirer le logo" }));
    await waitFor(() => expect(logos).toEqual([{ logo: png }, { logo: null }]));
    expect(await screen.findByText("Aucun logo pour le moment.")).toBeInTheDocument();
  });

  it("saves the owner's NIN and the social links as settings (CFG-001, CFG-004)", async () => {
    const saved: unknown[] = [];
    mockCommands({
      settings_get: ({ key }: Record<string, unknown>) => settingsValues[String(key)],
      settings_set: (args) => {
        saved.push(args);
        return args.value;
      },
      company_get: () => companyFixture,
      datapack_get: () => packFixture,
      company_update: () => companyFixture,
    });
    const user = userEvent.setup();
    renderWithProviders(<ShopSettings />);
    const nin = await screen.findByLabelText("NIN (18 chiffres)");
    await user.type(nin, "1234");
    await user.click(screen.getByRole("button", { name: "Enregistrer la boutique" }));
    expect(await screen.findByText("Le NIN doit contenir 18 chiffres.")).toBeInTheDocument();
    expect(saved).toHaveLength(0);
    await user.clear(nin);
    await user.type(nin, "١٠٩٨ ٧٦٥٤ ٣٢١٠ ٩٨٧٦ ٥٤");
    await user.type(screen.getByLabelText("Facebook"), "elamel.dz");
    // the activity is never written here: Settings › Activité et modules applies its template
    expect(screen.queryByLabelText("Modèle d'activité")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Enregistrer la boutique" }));
    await waitFor(() =>
      expect(saved).toEqual([
        { key: "company.nin", value: "109876543210987654" },
        { key: "company.social", value: { facebook: "elamel.dz" } },
      ]),
    );
  });
});

describe("Settings › Users and roles", () => {
  it("creates a cashier with a PIN", async () => {
    const created: unknown[] = [];
    mockCommands({
      ...settingsHandlers,
      users_list: () => [userFixture],
      roles_list: () => rolesFixture,
      users_create: (args) => {
        created.push(args);
        return userFixture;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<UsersSettings session={ownerSession} />);
    await user.click(await screen.findByRole("button", { name: "Ajouter un utilisateur" }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText("Nom affiché sur la tuile"), "Yacine");
    await user.type(within(dialog).getByLabelText("Code PIN (4 à 8 chiffres)"), "4826");
    await user.type(within(dialog).getByLabelText("Retapez le code PIN"), "4826");
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer l'utilisateur" }));
    await waitFor(() => expect(created).toHaveLength(1));
    expect(created[0]).toEqual({
      input: {
        username: null,
        displayName: "Yacine",
        pin: "4826",
        roleId: CASHIER_ROLE_ID,
        lang: "fr",
        discountLimitBp: 0,
      },
    });
  });

  it("groups permissions in plain words and locks the owner role", async () => {
    mockCommands({
      ...settingsHandlers,
      users_list: () => [],
      roles_list: () => rolesFixture,
      roles_catalog: () => catalogFixture,
    });
    const user = userEvent.setup();
    renderWithProviders(<UsersSettings session={ownerSession} />);
    const ownerRow = (await screen.findByText("Propriétaire")).closest("li");
    if (!ownerRow) throw new Error("owner role row missing");
    expect(within(ownerRow).getByText("1 utilisateur (1 actif)")).toBeInTheDocument();
    await user.click(within(ownerRow).getByRole("button", { name: /Modifier/ }));
    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByText("Le propriétaire peut tout faire. Ce rôle ne peut pas être modifié."),
    ).toBeInTheDocument();
    expect(await within(dialog).findByRole("group", { name: "Vente" })).toBeInTheDocument();
    expect(within(dialog).getByRole("checkbox", { name: "Vendre et encaisser" })).toBeDisabled();
  });

  it("names the row actions for screen readers and tooltips, with icons only", async () => {
    mockCommands({
      ...settingsHandlers,
      users_list: () => [{ ...userFixture, id: "u-other", displayName: "Yacine", isOwner: false }],
      roles_list: () => rolesFixture,
    });
    renderWithProviders(<UsersSettings session={ownerSession} />);
    const row = (await screen.findByText("Yacine")).closest("tr");
    if (!row) throw new Error("user row missing");
    // The reference PC is 1024 px wide: the labels no longer push the actions out of view.
    for (const name of ["Modifier", "Changer le code PIN", "Désactiver"]) {
      const button = within(row).getByRole("button", { name });
      expect(button).toHaveAttribute("title", name);
      expect(button).not.toHaveTextContent(name);
    }
  });

  it("lists the roles without the role editor for someone who is not the owner", async () => {
    mockCommands({
      ...settingsHandlers,
      users_list: () => [],
      roles_list: () => rolesFixture,
    });
    renderWithProviders(<UsersSettings session={{ ...ownerSession, isOwner: false }} />);
    const ownerRow = (await screen.findByText("Propriétaire")).closest("li");
    if (!ownerRow) throw new Error("owner role row missing");
    expect(within(ownerRow).getByText("1 utilisateur (1 actif)")).toBeInTheDocument();
    expect(within(ownerRow).queryByRole("button")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Nouveau rôle" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ajouter un utilisateur" })).toBeInTheDocument();
  });
});

describe("Settings › Backups", () => {
  /** What `backup_inspect` says of a sound backup this version can restore. */
  const validPreview = {
    status: "valid",
    schemaVersion: 7,
    appSchemaVersion: 7,
    companyName: "Supérette El Amel",
    lastTicket: null,
    documents: 0,
    products: 0,
    customers: 0,
    lastActivityAt: null,
    owners: [{ id: "u-owner", displayName: "Amine Benali" }],
  };

  it("backs up now and lists the file", async () => {
    let list: unknown[] = [];
    const calls = mockCommands({
      backup_list: () => list,
      backup_now: () => {
        list = [backupFixture];
        return backupFixture;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<BackupsSettings session={ownerSession} />);
    expect(
      await screen.findByText("Aucune sauvegarde pour le moment. Faites la première maintenant."),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Sauvegarder maintenant" }));
    expect(await screen.findByText("Manuelle")).toBeInTheDocument();
    expect(calls).toContain("backup_now");
  });

  it("restores only after the owner's PIN", async () => {
    const received: unknown[] = [];
    mockCommands({
      backup_list: () => [backupFixture],
      backup_inspect: () => validPreview,
      backup_restore: (args) => {
        received.push(args);
        return null;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<BackupsSettings session={ownerSession} />);
    await user.click(await screen.findByRole("button", { name: "Aperçu et restauration" }));
    const dialog = await screen.findByRole("dialog");
    await user.type(await within(dialog).findByLabelText("Votre code PIN de propriétaire"), "2580");
    await user.click(within(dialog).getByRole("button", { name: "Restaurer cette sauvegarde" }));
    await waitFor(() => expect(received).toEqual([{ fileName: backupFixture.fileName, pin: "2580" }]));
    expect(
      await within(dialog).findByText("Restauration… DZBuild POS redémarre dans un instant."),
    ).toBeInTheDocument();
  });

  it("shows someone who is not the owner the preview, never the restore", async () => {
    mockCommands({ backup_list: () => [backupFixture], backup_inspect: () => validPreview });
    const user = userEvent.setup();
    renderWithProviders(<BackupsSettings session={{ ...ownerSession, isOwner: false }} />);
    await user.click(await screen.findByRole("button", { name: "Aperçu et restauration" }));
    const dialog = await screen.findByRole("dialog");
    expect(
      await within(dialog).findByText("Seul le propriétaire peut restaurer une sauvegarde."),
    ).toBeInTheDocument();
    expect(within(dialog).queryByLabelText("Votre code PIN de propriétaire")).not.toBeInTheDocument();
    expect(
      within(dialog).queryByRole("button", { name: "Restaurer cette sauvegarde" }),
    ).not.toBeInTheDocument();
  });
});

describe("Settings › Audit journal", () => {
  it("shows readable actions and reports where the chain broke", async () => {
    mockCommands({
      audit_list: () => ({ rows: [auditRowFixture], nextBeforeSeq: null }),
      audit_verify_chain: () => ({ rows: 12, firstBrokenSeq: 7, lastHash: "0".repeat(64) }),
    });
    const user = userEvent.setup();
    renderWithProviders(<AuditSettings />);
    expect(await screen.findByText("Installation terminée")).toBeInTheDocument();
    expect(screen.getByText("Amine Benali")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Vérifier le journal" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("La ligne n° 7 du journal a été modifiée");
  });

  it("prints an Arabic row's date in reading order", async () => {
    await setLanguage("ar");
    mockCommands({ audit_list: () => ({ rows: [auditRowFixture], nextBeforeSeq: null }) });
    renderWithProviders(<AuditSettings />);
    // Intl's ar-DZ bidi marks used to reorder the characters inside this LTR <bdi>.
    const date = await screen.findByText("28/09/2026 13:00");
    expect(date.tagName).toBe("BDI");
    expect(date).toHaveAttribute("dir", "ltr");
    expect(date.textContent).toBe("28/09/2026 13:00");
  });

  it("confirms an intact chain", async () => {
    mockCommands({
      audit_list: () => ({ rows: [auditRowFixture], nextBeforeSeq: null }),
      audit_verify_chain: () => ({ rows: 12, firstBrokenSeq: null, lastHash: "f".repeat(64) }),
    });
    const user = userEvent.setup();
    renderWithProviders(<AuditSettings />);
    await user.click(await screen.findByRole("button", { name: "Vérifier le journal" }));
    expect(await screen.findByText("Lignes vérifiées : 12. Rien n'a été modifié.")).toBeInTheDocument();
  });
});
