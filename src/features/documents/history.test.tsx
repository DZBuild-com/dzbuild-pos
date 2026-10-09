import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SalesHistory } from "@/features/documents/SalesHistory";
import { AuditSettings } from "@/features/settings/AuditSettings";
import { EntityHistoryDialog } from "@/features/settings/EntityHistoryDialog";
import { setLanguage } from "@/lib/i18n";
import { auditRowFixture, cashierSession, ownerSession, ownerTile } from "@/test/fixtures";
import { documentFixture, priceLevelsFixture } from "@/test/fixtures-m1";
import { mockCommands, renderWithProviders } from "@/test/render";

const summary = {
  id: "d-1",
  docType: "ticket",
  number: "TK2026-C1-000123",
  status: "validated",
  businessDate: "2026-09-28",
  validatedAt: 1_790_593_200_000,
  totalDue: 30_500,
  userId: "u-cashier",
  terminalId: "t-1",
  parentId: null,
};

describe("receipts history", () => {
  beforeEach(() => {
    // jsdom has no layout: TanStack Virtual measures the table's scroller (offsetWidth/offsetHeight)
    // on mount, replacing `initialRect`, and would render no row at 0×0.
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1000);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
  });

  it("lists the receipts and reprints a DUPLICATA in the chosen language", async () => {
    const reprints: unknown[] = [];
    mockCommands({
      auth_list_users: () => [ownerTile, { ...ownerTile, id: "u-cashier", displayName: "Yacine" }],
      documents_list: () => [summary],
      documents_get: () => documentFixture,
      documents_reprint: (args) => {
        reprints.push(args);
        return { printer: "Caisse 1", bytes: 900, raster: true };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SalesHistory session={cashierSession} />, { route: "/sales" });
    const row = await screen.findByRole("row", { name: /TK2026-C1-000123/ });
    expect(within(row).getByText("Yacine")).toBeInTheDocument();
    expect(within(row).getByText(/305,00/)).toBeInTheDocument();
    await user.click(row);
    const sheet = await screen.findByRole("dialog", { name: "Ticket TK2026-C1-000123" });
    expect(within(sheet).getByText("Lait Candia 1 L")).toBeInTheDocument();
    await user.click(within(sheet).getByRole("radio", { name: "العربية" }));
    await user.click(within(sheet).getByRole("button", { name: "Réimprimer (DUPLICATA)" }));
    await waitFor(() => expect(reprints).toEqual([{ id: "d-1", lang: "ar" }]));
    expect(await within(sheet).findByText("Duplicata envoyé à Caisse 1")).toBeInTheDocument();
  });

  it("filters by number and date", async () => {
    const queries: unknown[] = [];
    mockCommands({
      auth_list_users: () => [ownerTile],
      documents_list: (args) => {
        queries.push(args);
        return [];
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SalesHistory session={ownerSession} />, { route: "/sales" });
    expect(await screen.findByText("Aucun ticket pour ces critères.")).toBeInTheDocument();
    await user.type(screen.getByLabelText("N° de ticket"), "TK2026-C1-0001");
    await waitFor(() =>
      expect(queries.at(-1)).toMatchObject({ query: { numberPrefix: "TK2026-C1-0001", limit: 100 } }),
    );
  });

  it("reads right to left in Arabic", async () => {
    await setLanguage("ar");
    mockCommands({ auth_list_users: () => [ownerTile], documents_list: () => [summary] });
    renderWithProviders(<SalesHistory session={cashierSession} />, { route: "/sales" });
    expect(await screen.findByRole("heading", { name: "التذاكر" })).toBeInTheDocument();
    expect(document.documentElement).toHaveAttribute("dir", "rtl");
  });
});

describe("audit and integrity", () => {
  it("filters the journal by family and checks tickets and closures", async () => {
    const histories: unknown[] = [];
    mockCommands({
      audit_list: () => ({ rows: [auditRowFixture], nextBeforeSeq: null }),
      audit_history: (args) => {
        histories.push(args);
        return {
          rows: [{ ...auditRowFixture, action: "drawer.opened", payload: { reason: "check" } }],
          nextBeforeSeq: null,
        };
      },
      audit_verify_chain: () => ({ rows: 5, firstBrokenSeq: null, lastSeq: 5, lastHash: "f".repeat(64) }),
      documents_verify_chain: () => [
        {
          docType: "ticket",
          series: "C1",
          rows: 123,
          firstBrokenSeq: null,
          firstBrokenNumber: null,
          lastSeq: 123,
          lastHash: "a".repeat(64),
        },
      ],
      cash_verify_closures: () => ({ rows: 31, firstBrokenZ: null, lastHash: "b".repeat(64) }),
      credit_verify_receipts: () => ({ rows: 4, firstBrokenNumber: null }),
      lan_verify_journals: () => ({ tills: [], mergeFirstBroken: null }),
    });
    const user = userEvent.setup();
    renderWithProviders(<AuditSettings />);
    await user.click(await screen.findByRole("radio", { name: "Tiroir" }));
    await waitFor(() => expect(histories.at(-1)).toMatchObject({ filter: { actionPrefix: "drawer." } }));
    expect(await screen.findByText("Tiroir-caisse ouvert sans vente")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Vérifier les tickets et les clôtures" }));
    expect(
      await screen.findByText("Ticket de caisse C1 : 123 documents vérifiés, rien n'a été modifié."),
    ).toBeInTheDocument();
    expect(screen.getByText("Clôtures Z : 31 vérifiées, rien n'a été modifié.")).toBeInTheDocument();
    expect(
      await screen.findByText("Reçus de versement : 4 vérifiés, rien n'a été modifié."),
    ).toBeInTheDocument();
  });

  it("names the first versement receipt that no longer matches the journal", async () => {
    mockCommands({
      audit_list: () => ({ rows: [], nextBeforeSeq: null }),
      documents_verify_chain: () => [],
      cash_verify_closures: () => ({ rows: 0, firstBrokenZ: null, lastHash: "0".repeat(64) }),
      credit_verify_receipts: () => ({ rows: 4, firstBrokenNumber: "RV2026-000002" }),
      lan_verify_journals: () => ({ tills: [], mergeFirstBroken: null }),
    });
    const user = userEvent.setup();
    renderWithProviders(<AuditSettings />);
    await user.click(await screen.findByRole("button", { name: "Vérifier les tickets et les clôtures" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Reçus de versement : modification détectée au reçu RV2026-000002. Contactez le support DZBuild.",
    );
  });

  it("on a main PC checks each till's journal and their merge chain too", async () => {
    const report = { rows: 12, broken: 0, firstBrokenSeq: null, lastSeq: 12, lastHash: "c".repeat(64) };
    mockCommands({
      audit_list: () => ({ rows: [], nextBeforeSeq: null }),
      documents_verify_chain: () => [],
      cash_verify_closures: () => ({ rows: 0, firstBrokenZ: null, lastHash: "0".repeat(64) }),
      credit_verify_receipts: () => ({ rows: 0, firstBrokenNumber: null }),
      lan_verify_journals: () => ({
        tills: [
          { terminalId: "t-c2", terminalCode: "C2", report },
          { terminalId: "t-c3", terminalCode: "C3", report: { ...report, broken: 1, firstBrokenSeq: 7 } },
        ],
        mergeFirstBroken: 40,
      }),
    });
    const user = userEvent.setup();
    renderWithProviders(<AuditSettings />);
    await user.click(await screen.findByRole("button", { name: "Vérifier les tickets et les clôtures" }));
    expect(
      await screen.findByText("Journal du poste C2 : 12 lignes vérifiées, rien n'a été modifié."),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Journal du poste C3 : modification trouvée à partir de la ligne 7. Contactez le support DZBuild.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Journaux des postes réunis sur ce poste principal : une modification a été trouvée. Contactez le support DZBuild.",
      ),
    ).toBeInTheDocument();
  });

  it("shows a product's history, field by field, in DZD (AUD-002)", async () => {
    mockCommands({
      auth_session: () => cashierSession,
      catalog_price_levels_list: () => priceLevelsFixture,
      audit_history: () => ({
        rows: [
          {
            ...auditRowFixture,
            seq: 10,
            action: "product.price_changed",
            entityType: "product",
            entityId: "p-oil",
            payload: { priceLevelId: "00000000-0000-7000-8000-000000000203", before: 65_000, after: 60_000 },
          },
          {
            ...auditRowFixture,
            seq: 9,
            action: "product.updated",
            entityType: "product",
            entityId: "p-oil",
            payload: {
              before: { sellPrice: 65_000, nameFr: "Huile Elio 5 L", costPrice: 40_000, minStock: 2_000 },
              after: { sellPrice: 68_000, nameFr: "Huile Elio 5 L", costPrice: 41_000, minStock: 5_000 },
            },
          },
        ],
        nextBeforeSeq: null,
      }),
    });
    renderWithProviders(
      <EntityHistoryDialog
        entityType="product"
        entityId="p-oil"
        title="Huile Elio 5 L"
        onClose={() => undefined}
      />,
    );
    const price = await screen.findByText("Prix de vente");
    const row = price.closest("tr") as HTMLElement;
    expect(row).toHaveTextContent("650,00 DA → 680,00 DA");
    expect(screen.getByText("Stock minimum").closest("tr")).toHaveTextContent("2 → 5");
    expect(screen.queryByText("sellPrice")).toBeNull();
    expect(screen.queryByText("Nom en français")).toBeNull();
    // The purchase price is for reports.profit only.
    expect(screen.queryByText("Prix d'achat (CMUP)")).toBeNull();
    // A level price change: its level by name, the two prices in DZD.
    const level = await screen.findByText("Prix Gros");
    expect(level.closest("tr")).toHaveTextContent("650,00 DA → 600,00 DA");
  });
});
