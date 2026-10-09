import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Toaster } from "sonner";
import { describe, expect, it } from "vitest";
import { BackupAlerts } from "@/app/BackupAlerts";
import { BackupsSettings } from "@/features/settings/BackupsSettings";
import {
  backupFixture,
  backupStatusFixture,
  cashierSession,
  ownerSession,
  settingsValues,
} from "@/test/fixtures";
import { lanTillStatusFixture } from "@/test/fixtures-m5";
import { mockCommands, renderWithProviders } from "@/test/render";

const base = {
  settings_get: ({ key }: Record<string, unknown>) => settingsValues[String(key)],
  settings_set: ({ value }: Record<string, unknown>) => value,
  backup_list: () => [backupFixture],
  backup_status: () => backupStatusFixture,
  backup_drives: () => [],
};

const preview = {
  status: "valid",
  schemaVersion: 3,
  appSchemaVersion: 3,
  companyName: "Supérette El Amel",
  lastTicket: { number: "TK2026-C1-000148", at: 1_790_593_200_000 },
  documents: 148,
  products: 512,
  customers: 37,
  lastActivityAt: 1_790_593_260_000,
  owners: [{ id: "u1", displayName: "Amine" }],
};

describe("Settings › Backups (M6)", () => {
  it("shows the shared schedule read-only on a till, with the main PC's name", async () => {
    mockCommands({
      ...base,
      backup_mirror_status: () => ({
        folder: null,
        label: null,
        available: false,
        freeBytes: null,
        lastCopyAt: null,
        lastError: null,
      }),
      lan_status: () => lanTillStatusFixture,
    });
    renderWithProviders(<BackupsSettings session={ownerSession} />);
    const card = await screen.findByRole("region", { name: "Quand sauvegarder" });
    expect(
      await within(card).findByText(
        "Lecture seule sur ce poste de caisse : faites-le sur le poste principal (Supérette El Amel).",
      ),
    ).toBeInTheDocument();
    expect(
      within(card).getByRole("switch", { name: "Sauvegarder quand l'application se ferme" }),
    ).toBeDisabled();
    expect(within(card).getByLabelText("Garder une copie par jour pendant")).toBeDisabled();
  });

  it("keeps the demo's copies away from the real shop's second copy", async () => {
    mockCommands({
      ...base,
      backup_mirror_status: () => ({
        folder: null,
        label: null,
        available: false,
        freeBytes: null,
        lastCopyAt: null,
        lastError: null,
      }),
      demo_status: () => ({ active: true, exists: true, ownerPin: "2580", cashierPin: "4826" }),
    });
    renderWithProviders(<BackupsSettings session={ownerSession} />);
    expect(
      await screen.findByText(/La boutique de démonstration garde ses sauvegardes dans son propre dossier/),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Choisir/ })).not.toBeInTheDocument();
  });

  it("shows a stick that is out and the last good copy, and copies to a stick that is in", async () => {
    const copied: string[] = [];
    mockCommands({
      ...base,
      backup_mirror_status: () => ({
        folder: "E:\\Sauvegardes POS",
        label: "KINGSTON",
        available: false,
        freeBytes: null,
        lastCopyAt: 1_790_593_200_000,
        lastError: "mirror_unavailable",
      }),
      backup_drives: () => [
        { mount: "F:\\", label: "SANDISK", freeBytes: 8_000_000_000, totalBytes: 16_000_000_000 },
      ],
      backup_to_drive: ({ mount }: Record<string, unknown>) => {
        copied.push(String(mount));
        return backupFixture;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(
      <>
        <BackupsSettings session={ownerSession} />
        <Toaster />
      </>,
    );
    const card = await screen.findByRole("region", { name: "Copie de secours (clé USB ou dossier)" });
    expect(within(card).getByText("Clé absente")).toBeInTheDocument();
    expect(within(card).getByText(/Dernière copie :/)).toBeInTheDocument();
    expect(
      within(card).getByText(/La clé USB ou le dossier de la copie de secours n'est pas accessible/),
    ).toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: "Copier sur SANDISK (F:\\)" }));
    await waitFor(() => expect(copied).toEqual(["F:\\"]));
    expect(await screen.findByText("Sauvegarde copiée sur la clé SANDISK.")).toBeInTheDocument();
  });

  it("previews a backup and restores it only with the owner's PIN", async () => {
    const restored: unknown[] = [];
    mockCommands({
      ...base,
      backup_mirror_status: () => ({
        folder: null,
        label: null,
        available: false,
        freeBytes: null,
        lastCopyAt: null,
        lastError: null,
      }),
      backup_inspect: () => preview,
      backup_restore: (args) => {
        restored.push(args);
        return null;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<BackupsSettings session={ownerSession} />);
    await user.click(await screen.findByRole("button", { name: "Aperçu et restauration" }));
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByText("TK2026-C1-000148")).toBeInTheDocument();
    expect(within(dialog).getByText("148")).toBeInTheDocument();
    const restore = within(dialog).getByRole("button", { name: "Restaurer cette sauvegarde" });
    expect(restore).toBeDisabled();
    await user.type(within(dialog).getByLabelText("Votre code PIN de propriétaire"), "2580");
    await user.click(restore);
    await waitFor(() => expect(restored).toEqual([{ fileName: backupFixture.fileName, pin: "2580" }]));
  });

  it("refuses to restore a backup of a newer version and says why", async () => {
    mockCommands({
      ...base,
      backup_mirror_status: () => ({
        folder: null,
        label: null,
        available: false,
        freeBytes: null,
        lastCopyAt: null,
        lastError: null,
      }),
      backup_inspect: () => ({ ...preview, status: "newer_than_app", schemaVersion: 9, owners: [] }),
    });
    const user = userEvent.setup();
    renderWithProviders(<BackupsSettings session={ownerSession} />);
    await user.click(await screen.findByRole("button", { name: "Aperçu et restauration" }));
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByText(/version plus récente de DZBuild POS/)).toBeInTheDocument();
    expect(
      within(dialog).queryByRole("button", { name: "Restaurer cette sauvegarde" }),
    ).not.toBeInTheDocument();
    expect(within(dialog).queryByLabelText("Votre code PIN de propriétaire")).not.toBeInTheDocument();
  });

  it("saves the retention and the backup on close", async () => {
    const saved: unknown[] = [];
    mockCommands({
      ...base,
      backup_mirror_status: () => ({
        folder: null,
        label: null,
        available: false,
        freeBytes: null,
        lastCopyAt: null,
        lastError: null,
      }),
      settings_set: (args) => {
        saved.push(args);
        return (args as { value: unknown }).value;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<BackupsSettings session={ownerSession} />);
    await user.selectOptions(await screen.findByLabelText("Garder une copie par jour pendant"), "30");
    await user.click(screen.getByRole("switch", { name: "Sauvegarder quand l'application se ferme" }));
    await waitFor(() =>
      expect(saved).toEqual([
        { key: "backup.keep_daily", value: 30 },
        { key: "backup.on_close", value: false },
      ]),
    );
  });

  it("warns every user when the second copy failed", async () => {
    mockCommands({ backup_status: () => ({ ...backupStatusFixture, mirrorError: "mirror_unavailable" }) });
    renderWithProviders(<BackupAlerts session={cashierSession} />);
    expect(await screen.findByText("La copie de secours n'a pas pu être faite")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Ouvrir les sauvegardes" })).not.toBeInTheDocument();
  });

  it("checks the database on demand and says when it is damaged", async () => {
    const calls = mockCommands({
      ...base,
      backup_mirror_status: () => ({
        folder: null,
        label: null,
        available: false,
        freeBytes: null,
        lastCopyAt: null,
        lastError: null,
      }),
      backup_check_database: () => false,
    });
    const user = userEvent.setup();
    renderWithProviders(<BackupsSettings session={ownerSession} />);
    const card = await screen.findByRole("region", { name: "Vérifier les données de la boutique" });
    await user.click(within(card).getByRole("button", { name: "Vérifier maintenant" }));
    expect(await within(card).findByRole("alert")).toHaveTextContent(
      "Les données de la boutique sont endommagées. Arrêtez les ventes et restaurez la dernière bonne sauvegarde.",
    );
    expect(calls).toContain("backup_check_database");
  });

  it("exports the shop as a .dzpos file and says where it went", async () => {
    mockCommands({
      ...base,
      backup_mirror_status: () => ({
        folder: null,
        label: null,
        available: false,
        freeBytes: null,
        lastCopyAt: null,
        lastError: null,
      }),
      backup_export_file: () => "E:\\dzbuild-pos-2026-10-01.dzpos",
    });
    const user = userEvent.setup();
    renderWithProviders(
      <>
        <BackupsSettings session={ownerSession} />
        <Toaster />
      </>,
    );
    const card = await screen.findByRole("region", { name: "Exporter mes données" });
    expect(within(card).getByRole("link", { name: "Toutes les données en tableur" })).toHaveAttribute(
      "href",
      "/settings/data",
    );
    await user.click(within(card).getByRole("button", { name: "Exporter une sauvegarde (.dzpos)" }));
    expect(await screen.findByText("Sauvegarde exportée.")).toBeInTheDocument();
    expect(screen.getByText("E:\\dzbuild-pos-2026-10-01.dzpos")).toBeInTheDocument();
  });
});
