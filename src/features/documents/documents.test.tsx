import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Toaster } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithProviders, mockCommands } from "@/test/render";
import { ownerSession, cashierSession } from "@/test/fixtures";
import { archivePageFixture, validatedInvoiceFixture, docStateFixture } from "@/test/fixtures-m3";
import invoiceFr from "@/documents/__fixtures__/invoice.fr.json";
import { setLanguage } from "@/lib/i18n";
import { DocumentsPage } from "./DocumentsPage";
import { DocumentView } from "./DocumentView";

beforeEach(() => {
  // jsdom has no layout: TanStack Virtual shows no row at 0×0, so every element measures 1024×600.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

describe("documents list", () => {
  it("lists the archive with states and filters by kind", async () => {
    const calls: Array<{ query: { kinds: string[] } }> = [];
    mockCommands({
      documents_search: (args) => {
        calls.push(args as { query: { kinds: string[] } });
        return archivePageFixture;
      },
    });
    renderWithProviders(<DocumentsPage session={ownerSession} />);
    expect(await screen.findByText("FA2026-000045")).toBeInTheDocument();
    // The amount sits in its own <bdi dir="ltr"> inside the badge: the badge's whole text is read.
    const unpaid = screen.getByText((_, element) => element?.getAttribute("data-state") === "unpaid");
    expect(unpaid.textContent).toBe("Reste dû 18\u202F000,00\u00A0DA");
    await userEvent.click(screen.getByRole("button", { name: "Devis" }));
    await waitFor(() => expect(calls.at(-1)?.query.kinds).toEqual(["quote"]));
  });

  it("offers only the kinds a cashier may make", async () => {
    mockCommands({ documents_search: () => ({ rows: [], next: null }) });
    renderWithProviders(<DocumentsPage session={cashierSession} />);
    expect(await screen.findByText("Aucun document pour ces filtres.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Nouveau document" })).not.toBeInTheDocument();
  });

  it("is right to left in Arabic", async () => {
    await setLanguage("ar");
    mockCommands({ documents_search: () => archivePageFixture });
    renderWithProviders(<DocumentsPage session={ownerSession} />);
    expect(await screen.findByText("FA2026-000045")).toBeInTheDocument();
    expect(document.documentElement.dir).toBe("rtl");
    await setLanguage("fr");
  });
});

describe("document view", () => {
  it("prints an original then says the next copy is a duplicate", async () => {
    const printed: unknown[] = [];
    mockCommands({
      documents_get: () => validatedInvoiceFixture,
      documents_state: () => docStateFixture,
      documents_links: () => ({ sources: [], targets: [] }),
      hardware_get: () => ({
        version: 1,
        printers: [],
        receiptPrinterId: null,
        receipt: {},
        scanner: { layout: "us" },
        documents: { defaultPrinter: "HP LaserJet M110", kinds: [], marginsMm: 12 },
      }),
      hardware_discover: () => [],
      documents_print: (args) => {
        printed.push(args);
        return { printer: "HP LaserJet M110", copies: 1, duplicate: false, dialog: false };
      },
    });
    // The result is a toast: the app shell's toaster shows it.
    renderWithProviders(
      <>
        <DocumentView id="inv-1" session={ownerSession} />
        <Toaster />
      </>,
    );
    await userEvent.click(await screen.findByRole("button", { name: /Imprimer/ }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByDisplayValue("HP LaserJet M110")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Imprimer" }));
    await waitFor(() => expect(printed).toHaveLength(1));
    expect(await screen.findByText("Imprimé (original)")).toBeInTheDocument();
  });

  it("previews the page in the dialog without printing", async () => {
    const calls = mockCommands({
      documents_get: () => validatedInvoiceFixture,
      documents_state: () => docStateFixture,
      documents_links: () => ({ sources: [], targets: [] }),
      documents_print_model: () => ({ template: "document", model: invoiceFr }),
    });
    renderWithProviders(<DocumentView id="inv-1" session={ownerSession} />);
    await userEvent.click(await screen.findByRole("button", { name: "Aperçu" }));
    expect(
      await screen.findByText(
        "Arrêtée la présente facture à la somme de : vingt-trois mille quatre cent quarante-neuf dinars",
      ),
    ).toBeInTheDocument();
    expect(calls).not.toContain("documents_print");
  });
});
