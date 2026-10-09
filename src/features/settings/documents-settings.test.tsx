import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { allowedSections } from "@/features/settings/sections";
import { i18n, setLanguage } from "@/lib/i18n";
import { cashierSession, ownerSession } from "@/test/fixtures";
import { categoriesFixture } from "@/test/fixtures-m1";
import { lanTillStatusFixture } from "@/test/fixtures-m5";
import { missingKeys, mockCommands, renderWithProviders } from "@/test/render";
import { DocumentsSettings } from "./DocumentsSettings";

const layout = {
  accent: "brand",
  showLogo: true,
  paper: "a4",
  secondLang: null,
  showDeliveryPrices: true,
  showStampBox: true,
};
const texts = {
  header: { ar: "", fr: "", en: "" },
  footer: { ar: "", fr: "", en: "" },
  terms: { ar: "", fr: "", en: "" },
  warranty: { ar: "", fr: "", en: "" },
};
const values: Record<string, unknown> = {
  "documents.layout": layout,
  "documents.texts": texts,
  "company.activity": { ar: "", fr: "", en: "" },
  "company.ae_card": "",
  "documents.quote_validity_days": 30,
  "documents.invoice_due_days": 30,
  "documents.arabic_guard": "consumer",
  "documents.arabic_guard_categories": [],
  "labels.arabic_categories": null,
};
const hardware = {
  version: 1,
  printers: [],
  receiptPrinterId: null,
  receipt: {},
  scanner: { layout: "us" },
  documents: { defaultPrinter: null, kinds: [], marginsMm: 12 },
};

/** A card by its title. Text queries limited to titles: role queries over the whole screen (six
 * cards and an A4 sheet) are slow in jsdom, too slow for a loaded PC running the full suite. */
async function cardTitled(title: string): Promise<HTMLElement> {
  const heading = await screen.findByText(title, { selector: "h2" }, { timeout: 10_000 });
  return heading.closest("section") as HTMLElement;
}

// Every test renders the whole screen with its A4 sample sheet: under a full parallel run on a
// loaded PC that outlives the suite's 15 s.
describe("document settings", { timeout: 30_000 }, () => {
  it("is read-only on a till, with the main PC's name and no save button", async () => {
    mockCommands({
      settings_get: ({ key }) => values[key as string],
      hardware_get: () => hardware,
      hardware_discover: () => [],
      catalog_categories_list: () => [],
      lan_status: () => lanTillStatusFixture,
    });
    renderWithProviders(<DocumentsSettings />);
    expect(
      await screen.findByText(
        "Lecture seule sur ce poste de caisse : faites-le sur le poste principal (Supérette El Amel).",
        {},
        { timeout: 10_000 },
      ),
    ).toBeInTheDocument();
    await cardTitled("Présentation");
    expect(screen.queryByRole("button", { name: /^Enregistrer/ })).not.toBeInTheDocument();
  });

  it("saves the layout and shows it in the preview", async () => {
    const set: Array<{ key: string; value: unknown }> = [];
    mockCommands({
      settings_get: ({ key }) => values[key as string],
      settings_set: (args) => {
        set.push(args as { key: string; value: unknown });
        return (args as { value: unknown }).value;
      },
      hardware_get: () => hardware,
      hardware_discover: () => [],
      catalog_categories_list: () => [],
    });
    renderWithProviders(<DocumentsSettings />);
    const card = await cardTitled("Présentation");
    await userEvent.click(within(card).getByRole("radio", { name: "Encre" }));
    await userEvent.click(within(card).getByRole("radio", { name: "A5" }));
    await userEvent.click(within(card).getByRole("button", { name: "Enregistrer la présentation" }));
    await waitFor(() => expect(set).toHaveLength(1));
    expect(set[0]).toMatchObject({ key: "documents.layout", value: { accent: "ink", paper: "a5" } });
    expect(screen.getByTestId("document-preview").querySelector("article")).toHaveAttribute(
      "data-accent",
      "ink",
    );
  });

  it("keeps a text within its limit and saves the three languages", async () => {
    const set: Array<{ key: string; value: unknown }> = [];
    mockCommands({
      settings_get: ({ key }) => values[key as string],
      settings_set: (args) => {
        set.push(args as { key: string; value: unknown });
        return (args as { value: unknown }).value;
      },
      hardware_get: () => hardware,
      hardware_discover: () => [],
      catalog_categories_list: () => [],
    });
    renderWithProviders(<DocumentsSettings />);
    const card = await cardTitled("Textes des documents");
    await userEvent.type(within(card).getByLabelText("Pied de page (Français)"), "Merci de votre confiance");
    expect(within(card).getByText("24 / 200")).toBeInTheDocument();
    await userEvent.click(within(card).getByRole("button", { name: "Enregistrer les textes" }));
    await waitFor(() =>
      expect(set[0]).toMatchObject({
        key: "documents.texts",
        value: { footer: { fr: "Merci de votre confiance", ar: "", en: "" } },
      }),
    );
  });

  it("chooses the A4 printer of invoices for this PC", async () => {
    const saved: unknown[] = [];
    mockCommands({
      auth_session: () => ownerSession,
      settings_get: ({ key }) => values[key as string],
      hardware_get: () => hardware,
      hardware_discover: () => [
        {
          kind: "windows_printer",
          name: "HP LaserJet M110",
          port: "USB001",
          driver: "HP",
          isDefault: true,
          offline: false,
          likelyReceipt: false,
        },
      ],
      hardware_save: (args) => {
        saved.push(args);
        return (args as { config: unknown }).config;
      },
      catalog_categories_list: () => [],
    });
    renderWithProviders(<DocumentsSettings />);
    const card = await cardTitled("Imprimantes A4 de ce poste");
    await userEvent.selectOptions(within(card).getByLabelText("Imprimante — Facture"), "HP LaserJet M110");
    await userEvent.clear(within(card).getByLabelText("Copies — Facture"));
    await userEvent.type(within(card).getByLabelText("Copies — Facture"), "2");
    await userEvent.click(within(card).getByRole("button", { name: "Enregistrer les imprimantes" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({
      config: {
        documents: { kinds: [{ docType: "invoice", printer: "HP LaserJet M110", paper: "a4", copies: 2 }] },
      },
    });
  });

  it("keeps the Arabic of labels and documents in one section and saves both guards", async () => {
    const set: Array<{ key: string; value: unknown }> = [];
    mockCommands({
      settings_get: ({ key }) => values[key as string],
      settings_set: (args) => {
        set.push(args as { key: string; value: unknown });
        return (args as { value: unknown }).value;
      },
      hardware_get: () => hardware,
      hardware_discover: () => [],
      catalog_categories_list: () => categoriesFixture,
    });
    const user = userEvent.setup();
    renderWithProviders(<DocumentsSettings />);
    const card = await cardTitled("Arabe sur les étiquettes et les documents");
    await user.click(within(card).getByRole("radio", { name: "Signaler pour les catégories choisies" }));
    await user.click(
      within(within(card).getByRole("group", { name: "Catégories des documents" })).getByRole("checkbox", {
        name: "Crèmerie",
      }),
    );
    await user.click(within(card).getByRole("radio", { name: "Seulement les catégories choisies" }));
    await user.click(
      within(within(card).getByRole("group", { name: "Catégories des étiquettes" })).getByRole("checkbox", {
        name: "Épicerie",
      }),
    );
    await user.click(within(card).getByRole("button", { name: "Enregistrer les règles de l'arabe" }));
    await waitFor(() => expect(set).toHaveLength(3));
    expect(set).toEqual([
      { key: "documents.arabic_guard", value: "categories" },
      { key: "documents.arabic_guard_categories", value: ["c-dairy"] },
      { key: "labels.arabic_categories", value: ["c-grocery"] },
    ]);
  });

  it("shows the backend's refusal on the field it concerns", async () => {
    mockCommands({
      settings_get: ({ key }) => values[key as string],
      settings_set: () => Promise.reject({ code: "value_invalid", field: "value" }),
      hardware_get: () => hardware,
      hardware_discover: () => [],
      catalog_categories_list: () => [],
    });
    const user = userEvent.setup();
    renderWithProviders(<DocumentsSettings />);
    const card = await cardTitled("Identité fiscale");
    await user.type(within(card).getByLabelText("Carte d'auto-entrepreneur"), "AE-16-000123");
    await user.click(within(card).getByRole("button", { name: "Enregistrer l'identité" }));
    expect(
      await within(card).findByText("Cette valeur n'est pas autorisée pour ce réglage."),
    ).toBeInTheDocument();
  });

  it("leaves the printers of this PC to those who manage its hardware", async () => {
    const calls = mockCommands({
      auth_session: () => ({
        ...ownerSession,
        permissions: ownerSession.permissions.filter((p) => p !== "settings.hardware"),
      }),
      settings_get: ({ key }) => values[key as string],
      hardware_get: () => hardware,
      hardware_discover: () => [],
      catalog_categories_list: () => [],
    });
    renderWithProviders(<DocumentsSettings />);
    expect(await cardTitled("Présentation")).toBeInTheDocument();
    await waitFor(() => expect(calls).toContain("auth_session"));
    expect(screen.queryByText("Imprimantes A4 de ce poste", { selector: "h2" })).not.toBeInTheDocument();
    expect(calls).not.toContain("hardware_discover");
  });

  it.each(["ar", "en"] as const)("has every word of the screen in %s", async (lang) => {
    const shown: Record<string, unknown> = {
      ...values,
      "documents.arabic_guard": "categories",
      "labels.arabic_categories": [],
    };
    mockCommands({
      auth_session: () => ownerSession,
      settings_get: ({ key }) => shown[key as string],
      hardware_get: () => hardware,
      hardware_discover: () => [],
      catalog_categories_list: () => categoriesFixture,
    });
    await setLanguage(lang);
    missingKeys.clear();
    renderWithProviders(<DocumentsSettings />);
    expect(await cardTitled(i18n.t("settings:documents.printers.title"))).toBeInTheDocument();
    expect([...missingKeys]).toEqual([]);
  });

  it("is a section of the owner's settings, hidden from a cashier", () => {
    expect(allowedSections(ownerSession).map((s) => s.path)).toContain("documents");
    expect(allowedSections(cashierSession).map((s) => s.path)).not.toContain("documents");
  });
});
