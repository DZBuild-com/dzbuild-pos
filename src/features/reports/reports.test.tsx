import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router";
import { describe, expect, it, vi } from "vitest";
import { ReportsPage } from "@/features/reports/ReportsPage";
import { defaultGrouping, requestOf } from "@/features/reports/ReportFilters";
import { ReportView } from "@/features/reports/ReportView";
import { rangeOf, today } from "@/lib/dates";
import type { ReportInfo } from "@/lib/ipc/types";
import { managerSession, ownerTile } from "@/test/fixtures";
import { promotionFixture, reportViewFixture } from "@/test/fixtures-m2";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";

/** Part of `reports_catalog` in French: a sales report, a profit one the role may not run, an
 * "as of" stock report and the soldes declaration that needs its promotion. */
const catalog: ReportInfo[] = [
  {
    kind: "sales_by_product",
    group: "sales",
    title: "Ventes par produit",
    groupTitle: "Ventes",
    filters: ["terminal", "user", "customer", "category", "product"],
    dated: true,
    requiredFilter: null,
    allowed: true,
  },
  {
    kind: "profit_by_product",
    group: "profit",
    title: "Marge par produit",
    groupTitle: "Marges",
    filters: ["terminal", "user", "customer", "category", "product"],
    dated: true,
    requiredFilter: null,
    allowed: false,
  },
  {
    kind: "low_stock",
    group: "stock",
    title: "Stock bas",
    groupTitle: "Stock",
    filters: ["location"],
    dated: false,
    requiredFilter: null,
    allowed: true,
  },
  {
    kind: "promotion_declaration",
    group: "declarations",
    title: "Déclaration de soldes",
    groupTitle: "Déclarations",
    filters: ["promotion"],
    dated: false,
    requiredFilter: "promotion",
    allowed: true,
  },
];

function mockReports(extra: Record<string, Handler> = {}) {
  return mockCommands({
    auth_session: () => managerSession,
    auth_list_users: () => [ownerTile],
    reports_catalog: () => catalog,
    catalog_categories_list: () => [],
    ...extra,
  });
}

function renderReport(kind: string) {
  return renderWithProviders(
    <Routes>
      <Route path="/reports/:kind" element={<ReportView />} />
    </Routes>,
    { route: `/reports/${kind}` },
  );
}

/** jsdom has no layout: the virtual table shows rows once its scroller measures something. */
function giveLayout() {
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
}

describe("reports", () => {
  it("lists the reports the role may run by group and finds one by its name", async () => {
    mockReports();
    const user = userEvent.setup();
    renderWithProviders(<ReportsPage />, { route: "/reports" });
    const sales = await screen.findByRole("region", { name: "Ventes" });
    expect(within(sales).getByRole("link", { name: "Ventes par produit" })).toHaveAttribute(
      "href",
      "/reports/sales_by_product",
    );
    expect(screen.queryByText("Marge par produit")).toBeNull();
    expect(screen.queryByRole("region", { name: "Marges" })).toBeNull();
    await user.type(screen.getByRole("searchbox", { name: "Rechercher un rapport" }), "declaration");
    expect(screen.getByRole("link", { name: "Déclaration de soldes" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Ventes par produit" })).toBeNull();
    expect(screen.queryByRole("region", { name: "Ventes" })).toBeNull();
  });

  it("shows a report with its chart and table and exports what it ran", async () => {
    giveLayout();
    const runs: unknown[] = [];
    const exported: unknown[] = [];
    mockReports({
      reports_run: (args) => {
        runs.push(args.request);
        return reportViewFixture;
      },
      exports_save: (args) => {
        exported.push(args.request);
        return { path: "C:\\ventes.xlsx", rows: 2 };
      },
    });
    const user = userEvent.setup();
    renderReport("sales_by_product");
    const table = await screen.findByRole("table", { name: "Ventes par produit" });
    expect(await within(table).findByText("Huile 5 L")).toBeInTheDocument();
    expect(screen.getByRole("group", { name: /Net TTC/ })).toBeInTheDocument();
    const { from, to } = rangeOf("this_month", today());
    // the names come in the UI language; the export writes the language chosen in its menu
    expect(runs[0]).toEqual({ kind: "sales_by_product", from, to, lang: "fr" });
    await user.click(screen.getByRole("button", { name: "Excel (xlsx)" }));
    await waitFor(() => expect(exported).toHaveLength(1));
    expect(exported[0]).toEqual({
      target: { kind: "report", request: { kind: "sales_by_product", from, to } },
      format: "xlsx",
      lang: "fr",
    });
  });

  it("draws no chart when the report has none and says when the rows were cut", async () => {
    mockReports({
      reports_run: () => ({
        ...reportViewFixture,
        report: { ...reportViewFixture.report, chart: null, truncated: true },
      }),
    });
    renderReport("sales_by_product");
    expect(await screen.findByRole("table", { name: "Ventes par produit" })).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: /Net TTC/ })).toBeNull();
    expect(screen.getByText(/Seules les 2 premières lignes sont affichées/)).toBeInTheDocument();
  });

  it("asks for the promotion before running its declaration", async () => {
    const runs: unknown[] = [];
    const calls = mockReports({
      promotions_list: () => [promotionFixture],
      reports_run: (args) => {
        runs.push(args.request);
        return { ...reportViewFixture, title: "Déclaration de soldes" };
      },
    });
    const user = userEvent.setup();
    renderReport("promotion_declaration");
    const picker = await screen.findByRole("combobox", { name: "Promotion" });
    expect(screen.getByText("Choisissez la promotion pour afficher sa déclaration.")).toBeInTheDocument();
    expect(screen.queryByRole("table")).toBeNull();
    expect(calls).not.toContain("reports_run");
    await user.selectOptions(picker, await screen.findByRole("option", { name: "Remise fidèle" }));
    expect(await screen.findByRole("table", { name: "Déclaration de soldes" })).toBeInTheDocument();
    expect(runs[0]).toEqual({
      kind: "promotion_declaration",
      from: today(),
      to: today(),
      promotionId: "promo-1",
      lang: "fr",
    });
  });
});

describe("report grouping", () => {
  it("groups a long period by week or month until the user picks a grouping", () => {
    expect(defaultGrouping({ from: "2026-09-01", to: "2026-09-30" })).toBe("day");
    expect(defaultGrouping({ from: "2026-01-01", to: "2026-09-30" })).toBe("week");
    expect(defaultGrouping({ from: "2024-01-01", to: "2026-09-30" })).toBe("month");
    const info = { ...catalog[0]!, kind: "profit_by_period" as const, filters: ["grouping" as const] };
    const range = { from: "2026-01-01", to: "2026-09-30" };
    expect(requestOf(info, { range, asOf: range.to, values: {} })).toMatchObject({ group: "week" });
    expect(requestOf(info, { range, asOf: range.to, values: { grouping: "day" } })).toMatchObject({
      group: "day",
    });
  });
});
