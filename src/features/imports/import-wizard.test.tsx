import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ImportJobs } from "@/features/imports/ImportJobs";
import { ImportWizard } from "@/features/imports/ImportWizard";
import { DataSettings } from "@/features/settings/DataSettings";
import { importFileFixture, importJobFixture, importPreviewFixture } from "@/test/fixtures-m2";
import { managerSession, ownerSession, storekeeperSession } from "@/test/fixtures";
import { emitEvent, mockCommands, renderWithProviders } from "@/test/render";

beforeEach(() => {
  // jsdom has no layout: TanStack Virtual measures its scroller with offsetWidth/offsetHeight and
  // shows no row at 0×0, so every element measures 1024×600.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

describe("import wizard", () => {
  it("checks the file, shows the problems and imports only the checked mapping", async () => {
    const applied: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      imports_open_file: () => importFileFixture,
      imports_preview: () => importPreviewFixture,
      imports_dry_run: () => ({
        total: 3,
        create: 2,
        update: 0,
        skip: 0,
        fail: 1,
        archive: 0,
        problemsTruncated: false,
        fingerprint: "f",
        problems: [{ row: 4, field: "sellPrice", value: "1,234", code: "ambiguous_separator" }],
      }),
      imports_apply: (args) => {
        applied.push(args.options);
        emitEvent("import-progress", {
          jobId: "j1",
          done: 3,
          total: 3,
          created: 2,
          updated: 0,
          skipped: 0,
          failed: 1,
        });
        return {
          id: "j1",
          kind: "products",
          status: "done",
          created: 2,
          updated: 0,
          skipped: 0,
          failed: 1,
          total: 3,
          problems: [],
          undoable: true,
        };
      },
    });
    renderWithProviders(<ImportWizard entity="products" />);
    fireEvent.click(screen.getByRole("button", { name: "Choisir le fichier" }));
    fireEvent.click(await screen.findByRole("button", { name: "Suivant" }));
    expect(screen.getByRole("button", { name: "Importer" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Vérifier le fichier" }));
    expect(await screen.findByText(/Séparateur ambigu/)).toBeInTheDocument();
    expect(screen.getByText("Ligne 4")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Importer" }));
    await waitFor(() => expect(applied).toHaveLength(1));
    expect(await screen.findByText(/2 créés/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Annuler cet import" })).toBeEnabled();
  });

  it("asks for a new check when the mapping changes after a check", async () => {
    mockCommands({
      auth_session: () => managerSession,
      imports_open_file: () => importFileFixture,
      imports_preview: () => importPreviewFixture,
      imports_dry_run: () => ({
        total: 3,
        create: 3,
        update: 0,
        skip: 0,
        fail: 0,
        archive: 0,
        problems: [],
        problemsTruncated: false,
        fingerprint: "f",
      }),
    });
    renderWithProviders(<ImportWizard entity="products" />);
    fireEvent.click(screen.getByRole("button", { name: "Choisir le fichier" }));
    fireEvent.click(await screen.findByRole("button", { name: "Suivant" }));
    fireEvent.click(screen.getByRole("button", { name: "Vérifier le fichier" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Importer" })).toBeEnabled());
    fireEvent.change(screen.getByLabelText("Colonne 3"), { target: { value: "costPrice" } });
    expect(screen.getByRole("button", { name: "Importer" })).toBeDisabled();
  });

  it("sends the mapping, the policy and the file name it checked, and the template in the UI language", async () => {
    const checked: unknown[] = [];
    const templates: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      imports_template: (args) => {
        templates.push(args);
        return "C:\\Users\\caisse\\Documents\\template-products.xlsx";
      },
      imports_open_file: () => importFileFixture,
      imports_preview: () => importPreviewFixture,
      imports_dry_run: (args) => {
        checked.push(args);
        return {
          total: 3,
          create: 1,
          update: 2,
          skip: 0,
          fail: 0,
          archive: 4,
          stockKept: 1,
          stockKeptNames: ["P000012 Huile 5 L"],
          problems: [],
          problemsTruncated: false,
          fingerprint: "f",
        };
      },
    });
    renderWithProviders(<ImportWizard entity="products" />);
    fireEvent.click(screen.getByRole("button", { name: "Télécharger le modèle" }));
    await waitFor(() => expect(templates).toEqual([{ entity: "products", lang: "fr" }]));
    fireEvent.click(screen.getByRole("button", { name: "Choisir le fichier" }));
    fireEvent.click(await screen.findByRole("button", { name: "Suivant" }));
    // each column shows its title and its first values next to the field it feeds
    expect(screen.getByLabelText("Colonne 1")).toHaveValue("reference");
    expect(screen.getByText("Lait UHT 1 L")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Colonne 2"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("radio", { name: /^Aligner la liste sur le fichier/ }));
    fireEvent.click(screen.getByRole("button", { name: "Vérifier le fichier" }));
    await waitFor(() =>
      expect(checked).toEqual([
        {
          token: "t-articles",
          sheet: 0,
          options: {
            entity: "products",
            policy: "sync",
            fileName: "articles.csv",
            headerRow: 0,
            mapping: ["reference", null, "sellPrice"],
          },
        },
      ]),
    );
    // a sync also says how many products it would archive
    const archive = await screen.findByText("À archiver");
    expect(within(archive.closest("dl")!).getByText("4")).toBeInTheDocument();
    // and which products it keeps because they still hold stock
    expect(screen.getByText(/gardés pour leur stock \(non archivés\) : 1/)).toBeInTheDocument();
    expect(screen.getByText("P000012 Huile 5 L")).toBeInTheDocument();
  });
});

describe("import history", () => {
  it("undoes the latest import of the day after a confirmation", async () => {
    const undone: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      imports_jobs: () => [
        importJobFixture,
        {
          ...importJobFixture,
          id: "j-0",
          fileName: "clients.xlsx",
          kind: "customers",
          created: 40,
          failed: 0,
          undoable: false,
        },
      ],
      imports_undo: (args) => {
        undone.push(args);
        return { ...importJobFixture, status: "undone", undoable: false };
      },
    });
    renderWithProviders(<ImportJobs />);
    expect(await screen.findByText("clients.xlsx")).toBeInTheDocument();
    expect(screen.getByText("2 créés, 0 mis à jour, 0 ignorés, 1 refusés")).toBeInTheDocument();
    // only the latest import of the day can be undone
    expect(screen.getAllByRole("button", { name: "Annuler cet import" })).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Annuler cet import" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Annuler cet import" }));
    await waitFor(() => expect(undone).toEqual([{ jobId: "j-1" }]));
  });
});

describe("data settings", () => {
  it("exports the data and backup files, imports a backup file and links to the imports", async () => {
    const calls = mockCommands({
      auth_session: () => ownerSession,
      data_export_all: () => ({ path: "C:\\Users\\caisse\\dzbuild-pos-data-2026-09-30.zip", rows: 1_250 }),
      backup_export_file: () => "E:\\dzbuild-pos-2026-09-30.dzpos",
      backup_import_file: () => null,
    });
    renderWithProviders(<DataSettings />);
    fireEvent.click(screen.getByRole("button", { name: "Exporter toutes les données (zip)" }));
    await waitFor(() => expect(calls).toContain("data_export_all"));
    fireEvent.click(screen.getByRole("button", { name: "Exporter un fichier de sauvegarde (.dzpos)" }));
    await waitFor(() => expect(calls).toContain("backup_export_file"));
    fireEvent.click(screen.getByRole("button", { name: "Importer un fichier de sauvegarde" }));
    await waitFor(() => expect(calls).toContain("backup_import_file"));
    expect(await screen.findByRole("link", { name: "Produits" })).toHaveAttribute(
      "href",
      "/imports/products",
    );
    expect(screen.getByRole("link", { name: "Soldes d'ouverture des clients" })).toHaveAttribute(
      "href",
      "/imports/customer_balances",
    );
  });

  it("links only to the imports the role may run", async () => {
    mockCommands({ auth_session: () => storekeeperSession });
    renderWithProviders(<DataSettings />);
    expect(await screen.findByRole("link", { name: "Stock d'ouverture" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Clients" })).not.toBeInTheDocument();
  });
});

describe("import wizard title row", () => {
  it("reads the titles, samples and suggested mapping of the title row the user picks", async () => {
    const asked: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      imports_open_file: () => importFileFixture,
      imports_preview: (args) => {
        asked.push(args.headerRow);
        // A two-cell banner in row 1 (the reader took it for the titles), the titles in row 2.
        return args.headerRow === 1
          ? importPreviewFixture
          : {
              ...importPreviewFixture,
              headerRow: 0,
              headers: ["Inventaire du 28/09", "Magasin"],
              sample: [["Référence", "Désignation", "Prix de vente"], ...importPreviewFixture.sample],
              suggested: [null, null, null],
            };
      },
    });
    renderWithProviders(<ImportWizard entity="products" />);
    fireEvent.click(screen.getByRole("button", { name: "Choisir le fichier" }));
    fireEvent.click(await screen.findByRole("button", { name: "Suivant" }));
    expect(screen.getByText("Inventaire du 28/09")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Colonne 1"), { target: { value: "costPrice" } });
    fireEvent.change(screen.getByLabelText("Ligne des titres"), { target: { value: "1" } });
    await waitFor(() => expect(screen.queryByText("Inventaire du 28/09")).not.toBeInTheDocument());
    expect(screen.getByText("Désignation")).toBeInTheDocument();
    expect(asked).toEqual([null, 1]);
    // The suggestion for the new titles, not the choice made against the banner.
    expect(screen.getByLabelText("Colonne 2")).toHaveValue("nameFr");
    expect(screen.getByLabelText("Colonne 1")).toHaveValue("reference");
  });
});
