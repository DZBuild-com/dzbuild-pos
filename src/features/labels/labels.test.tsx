import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Toaster } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CollectorImportButton } from "@/features/stock/CollectorImportButton";
import { LabelDesigner } from "@/features/labels/LabelDesigner";
import { PrintLabelsDialog } from "@/features/labels/PrintLabelsDialog";
import { ScaleItemsTab } from "@/features/labels/ScaleItemsTab";
import { StaleLabelsTab } from "@/features/labels/StaleLabelsTab";
import {
  hardwareV2Fixture,
  labelBatchFixture,
  labelSheetFixture,
  labelTemplatesFixture,
  scaleItemsFixture,
  staleLabelsFixture,
} from "@/test/fixtures-m4";
import { lanTillStatusFixture } from "@/test/fixtures-m5";
import { mockCommands, renderWithProviders } from "@/test/render";
import { ownerSession } from "@/test/fixtures";

const png = "data:image/png;base64,iVBORw0KGgo=";
const withLabelPrinter = {
  ...hardwareV2Fixture,
  labelPrinters: [
    {
      id: "l1",
      name: "Étiquettes",
      transport: { kind: "windows", printerName: "XP-365B" },
      profile: {
        preset: "xprinter",
        language: "tspl",
        dpi: "d203",
        darkness: 8,
        speed: 4,
        media: "gap",
        offsetXDmm: 0,
        offsetYDmm: 0,
        bitmapBlackIsZero: true,
      },
    },
  ],
  labelPrinterId: "l1",
};

beforeEach(() => {
  // jsdom has no layout: TanStack Virtual measures the DataTable's scroller with
  // offsetWidth/offsetHeight and shows no row at 0×0.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

describe("labels", () => {
  it("shows the templates read-only on a till, with no save or new button", async () => {
    mockCommands({
      labels_templates: () => labelTemplatesFixture,
      catalog_categories_list: () => [],
      labels_preview: () => ({ png, label: labelBatchFixture.groups[0]!.labels[0], dpi: "d203" }),
      lan_status: () => lanTillStatusFixture,
    });
    renderWithProviders(<LabelDesigner />);
    expect(
      await screen.findByText(
        "Lecture seule sur ce poste de caisse : faites-le sur le poste principal (Supérette El Amel).",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Enregistrer les modèles" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Nouveau modèle" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("Taille")).toBeDisabled();
  });

  it("shows the Arabic warning and prints French-only labels only after confirming", async () => {
    const sent: Array<Record<string, unknown>> = [];
    mockCommands({
      labels_templates: () => labelTemplatesFixture,
      labels_build: () => labelBatchFixture,
      labels_preview: () => ({ png, label: labelBatchFixture.groups[0]!.labels[0], dpi: "d203" }),
      hardware_get: () => withLabelPrinter,
      auth_session: () => ownerSession,
      labels_print: (args) => {
        sent.push(args.input as Record<string, unknown>);
        return { outcome: { printer: "Étiquettes", bytes: 100, raster: true }, labels: 2, assigned: [] };
      },
    });
    renderWithProviders(
      <PrintLabelsDialog
        source={{ kind: "products", items: [{ productId: "p-milk", copies: 2, lotId: null }] }}
        onClose={() => {}}
      />,
    );
    expect(await screen.findByRole("alert")).toHaveTextContent("arabe");
    await userEvent.click(screen.getByRole("button", { name: "Imprimer sans l'arabe" }));
    await userEvent.click(await screen.findByRole("button", { name: "Oui, imprimer en français seulement" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({ arabicOverride: true });
    expect(await screen.findByRole("status")).toHaveTextContent("2");
  });

  it("puts the labels on an A4 sheet from a start position and prints it through the print host", async () => {
    const asked: Array<Record<string, unknown>> = [];
    const printed: Array<Record<string, unknown>> = [];
    let hostUp = false;
    mockCommands({
      labels_templates: () => labelTemplatesFixture,
      labels_build: () => labelBatchFixture,
      labels_preview: () => ({ png, label: labelBatchFixture.groups[0]!.labels[0], dpi: "d203" }),
      hardware_get: () => ({
        ...withLabelPrinter,
        documents: { defaultPrinter: "HP LaserJet M110", kinds: [], marginsMm: 12 },
      }),
      auth_session: () => ownerSession,
      labels_sheet: (args) => {
        const input = args.input as { start: number };
        asked.push(input);
        return labelSheetFixture(input.start);
      },
      labels_sheet_print: (args) => {
        printed.push(args.input as Record<string, unknown>);
        return hostUp
          ? { printer: "HP LaserJet M110", copies: 1, duplicate: false, dialog: false }
          : Promise.reject({ code: "print_host_unavailable" });
      },
    });
    renderWithProviders(
      <PrintLabelsDialog
        source={{ kind: "products", items: [{ productId: "p-milk", copies: 2, lotId: null }] }}
        onClose={() => {}}
      />,
    );
    await userEvent.click(await screen.findByRole("button", { name: "Planche A4…" }));
    expect(await screen.findByRole("heading", { name: "Planche d'étiquettes A4" })).toBeInTheDocument();
    // French-only labels of a guarded product: no sheet before the user answers (DOC-034)
    expect(await screen.findByRole("alert")).toHaveTextContent("arabe");
    expect(asked).toHaveLength(0);
    await userEvent.click(screen.getByRole("button", { name: "Imprimer sans l'arabe" }));
    await userEvent.click(await screen.findByRole("button", { name: "Oui, imprimer en français seulement" }));
    expect(await screen.findByTestId("sheet-preview")).toBeInTheDocument();
    expect(asked[0]).toMatchObject({ formatId: "a4_24_70x37", start: 1, arabicOverride: true });
    expect(screen.getByText("HP LaserJet M110")).toBeInTheDocument();

    // a partly used sheet: the two labels go to positions 23 and 24
    await userEvent.click(screen.getByRole("button", { name: "23" }));
    await waitFor(() => expect(asked.at(-1)).toMatchObject({ start: 23 }));
    expect(screen.getByRole("button", { name: "23" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "22" })).toHaveClass("line-through");
    expect(screen.getByRole("button", { name: "24" })).toHaveClass("bg-brand-soft");

    // the print host fails: the error, and nothing said printed (Rust journals only a success)
    await userEvent.click(screen.getByRole("button", { name: "Imprimer la planche" }));
    expect(await screen.findByText(/L'impression A4 n'a pas pu démarrer/)).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    hostUp = true;
    await userEvent.click(screen.getByRole("button", { name: "Imprimer la planche" }));
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Planche d'étiquettes envoyée à l'imprimante.",
    );
    expect(printed).toHaveLength(2);
    expect(printed[1]).toMatchObject({
      request: { source: { kind: "products", items: [{ productId: "p-milk", copies: 2 }] } },
      formatId: "a4_24_70x37",
      start: 23,
      arabicOverride: true,
    });

    // another planche starts again at its first position
    await userEvent.selectOptions(screen.getByLabelText("Planche"), "a4_65_38x21");
    await waitFor(() => expect(asked.at(-1)).toMatchObject({ formatId: "a4_65_38x21", start: 1 }));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("previews an unsaved template and saves it with its category", async () => {
    const previews: unknown[] = [];
    const saved: unknown[] = [];
    mockCommands({
      labels_templates: () => labelTemplatesFixture,
      catalog_categories_list: () => [
        {
          id: "c-dairy",
          parentId: null,
          nameAr: "ألبان",
          nameFr: "Crèmerie",
          nameEn: "Dairy",
          color: null,
          sortOrder: 0,
          image: null,
          archived: false,
        },
      ],
      labels_preview: (args) => {
        previews.push(args.input);
        return { png, label: labelBatchFixture.groups[0]!.labels[0], dpi: "d203" };
      },
      labels_save_templates: (args) => {
        saved.push(args);
        return labelTemplatesFixture;
      },
    });
    renderWithProviders(<LabelDesigner />);
    await userEvent.click(await screen.findByRole("button", { name: /Prix 40 × 25/ }));
    await userEvent.selectOptions(screen.getByLabelText("Taille"), "50x30");
    await waitFor(() =>
      expect(previews.at(-1)).toMatchObject({ template: { widthDmm: 500, heightDmm: 300 } }),
    );
    expect(screen.getByRole("img", { name: "Aperçu de l'étiquette" })).toHaveAttribute("src", png);
    await userEvent.selectOptions(screen.getByLabelText("Crèmerie"), "builtin.price_40x25");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les modèles" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({ categoryTemplates: { "c-dairy": "builtin.price_40x25" } });
  });

  it("says a preset size is too small where it is chosen, and where to fix it when printing", async () => {
    const tooSmall = {
      code: "printer_setting_invalid",
      field: "template.heightDmm",
      message: "template.heightDmm",
    };
    mockCommands({
      labels_templates: () => labelTemplatesFixture,
      catalog_categories_list: () => [],
      labels_preview: () => Promise.reject(tooSmall),
    });
    const { unmount } = renderWithProviders(<LabelDesigner />);
    await userEvent.click(await screen.findByRole("button", { name: /Prix 40 × 25/ }));
    const size = screen.getByLabelText("Taille");
    // the refusal is on the size the user chose, not lost with the preview
    await waitFor(() => expect(within(size.closest("div")!).getByRole("alert")).toBeInTheDocument());
    unmount();
    mockCommands({
      labels_templates: () => labelTemplatesFixture,
      labels_build: () => ({ ...labelBatchFixture, warnings: [] }),
      labels_preview: () => ({ png, label: labelBatchFixture.groups[0]!.labels[0], dpi: "d203" }),
      hardware_get: () => withLabelPrinter,
      auth_session: () => ownerSession,
      labels_print: () => Promise.reject(tooSmall),
    });
    renderWithProviders(
      <PrintLabelsDialog
        source={{ kind: "products", items: [{ productId: "p-milk", copies: 2, lotId: null }] }}
        onClose={() => {}}
      />,
    );
    const print = await screen.findByRole("button", { name: /Imprimer \d* ?étiquettes?/ });
    await waitFor(() => expect(print).toBeEnabled());
    await userEvent.click(print);
    expect(
      await screen.findByText(
        "Le modèle d'étiquette est trop petit pour ce qu'il imprime : choisissez une plus grande taille dans Étiquettes › Modèles.",
      ),
    ).toBeInTheDocument();
  });

  it("prints or dismisses the labels to reprint", async () => {
    const dismissed: unknown[] = [];
    mockCommands({
      labels_stale: () => staleLabelsFixture,
      labels_dismiss: (args) => {
        dismissed.push(args.productIds);
        return 1;
      },
    });
    renderWithProviders(<StaleLabelsTab />);
    const row = await screen.findByRole("row", { name: /Lait Candia/ });
    expect(within(row).getByText("Prix changé")).toBeInTheDocument();
    await userEvent.click(within(row).getByRole("checkbox"));
    await userEvent.click(screen.getByRole("button", { name: "Marquer à jour" }));
    await waitFor(() => expect(dismissed).toEqual([["p-milk"]]));
  });

  it("edits scale items inline and exports the PLU list", async () => {
    const set: unknown[] = [];
    const exported: unknown[] = [];
    mockCommands({
      scale_items_list: () => scaleItemsFixture,
      scale_items_set: (args) => {
        set.push(args);
        return scaleItemsFixture[0];
      },
      scale_plu_export: (args) => {
        exported.push(args);
        return "plu-2026-09-29.xlsx";
      },
    });
    // the saved file name is a toast
    renderWithProviders(
      <>
        <ScaleItemsTab />
        <Toaster />
      </>,
    );
    const tare = await screen.findByLabelText("Tare (g) — Tomates");
    await userEvent.clear(tare);
    await userEvent.type(tare, "150");
    await userEvent.tab();
    await waitFor(() =>
      expect(set).toEqual([{ productId: "p-tomato", input: { hotkey: 3, tareG: 150, shelfLifeDays: 3 } }]),
    );
    await userEvent.click(screen.getByRole("button", { name: "Exporter la liste PLU (Excel)" }));
    await waitFor(() => expect(exported).toEqual([{ format: "xlsx", lang: "fr" }]));
    expect(await screen.findByText(/plu-2026-09-29.xlsx/)).toBeInTheDocument();
  });

  it("shows the scale items read-only on a till, with the main PC's name", async () => {
    mockCommands({
      lan_status: () => lanTillStatusFixture,
      scale_items_list: () => scaleItemsFixture,
    });
    renderWithProviders(<ScaleItemsTab />);
    expect(
      await screen.findByText(
        "Lecture seule sur ce poste de caisse : faites-le sur le poste principal (Supérette El Amel).",
      ),
    ).toBeInTheDocument();
    expect(await screen.findByLabelText("Tare (g) — Tomates")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Exporter la liste PLU (Excel)" })).toBeEnabled();
  });

  it("shows a collector file's unknown codes and bad lines", async () => {
    mockCommands({
      counts_import_collector: () => ({
        matched: 3,
        unknown: ["999"],
        problems: [{ line: 4, code: "qty_invalid" }],
      }),
    });
    renderWithProviders(<CollectorImportButton countId="k1" />);
    await userEvent.click(screen.getByRole("button", { name: "Importer un fichier de terminal" }));
    expect(await screen.findByText(/3 lignes comptées/)).toBeInTheDocument();
    expect(screen.getByText("999")).toBeInTheDocument();
    expect(screen.getByText(/Ligne 4/)).toBeInTheDocument();
  });
});
