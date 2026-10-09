import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DashboardPage } from "@/features/dashboard/DashboardPage";
import type { ReportInfo, ReportKind } from "@/lib/ipc/types";
import { dashboardFixture } from "@/test/fixtures-m2";
import { cashierSession, managerSession } from "@/test/fixtures";
import { mockCommands, renderWithProviders } from "@/test/render";

describe("dashboard", () => {
  it("shows today's figures and leaves out what the backend hid", async () => {
    mockCommands({
      auth_session: () => managerSession,
      reports_dashboard: () => ({ ...dashboardFixture, margin: null, payables: null }),
    });
    renderWithProviders(<DashboardPage />);
    expect(await screen.findByText("Ventes du jour")).toBeInTheDocument();
    expect(screen.getByText(/2\s?380,00/)).toBeInTheDocument();
    expect(screen.queryByText("Marge du jour")).not.toBeInTheDocument();
    expect(screen.queryByText("Ce que vous devez aux fournisseurs")).not.toBeInTheDocument();
    expect(screen.getByRole("group", { name: /14 derniers jours/ })).toBeInTheDocument();
  });

  it("gives a money tile room for an eight-figure month on a 1366 px screen", async () => {
    mockCommands({
      auth_session: () => managerSession,
      reports_dashboard: () => ({ ...dashboardFixture, monthNetTtc: 1_234_567_890 }),
    });
    renderWithProviders(<DashboardPage />);
    const tile = (await screen.findByText("Mois en cours")).closest("dl") as HTMLElement;
    expect(within(tile).getByText(/12\s?345\s?678,90/)).toBeInTheDocument();
    // Tiles are never packed below 14rem (224 px): 1366 px less the side bar holds four of them,
    // wide enough for "12 345 678,90 DA" at the smaller size used below xl; a longer amount wraps
    // inside its tile instead of spilling into the next one.
    expect(screen.getByTestId("dashboard-tiles").className).toContain(
      "grid-cols-[repeat(auto-fill,minmax(14rem,1fr))]",
    );
    const value = within(tile).getAllByRole("definition")[0] as HTMLElement;
    const classes = (el: HTMLElement) => el.className.split(/\s+/);
    expect(classes(value)).toEqual(expect.arrayContaining(["min-w-0", "text-xl", "xl:text-2xl"]));
    expect(classes(tile)).toContain("min-w-0");
  });

  it("is not offered to a cashier", async () => {
    mockCommands({ auth_session: () => cashierSession });
    renderWithProviders(<DashboardPage />);
    expect(await screen.findByText(/pas accès/)).toBeInTheDocument();
  });

  it("opens the report behind a figure when the role may run it", async () => {
    const info = (kind: ReportKind, allowed: boolean): ReportInfo => ({
      kind,
      group: "sales",
      title: kind,
      groupTitle: "Ventes",
      filters: [],
      dated: true,
      requiredFilter: null,
      allowed,
    });
    mockCommands({
      auth_session: () => managerSession,
      reports_dashboard: () => dashboardFixture,
      reports_catalog: () => [info("sales_summary", true), info("returns", false)],
    });
    renderWithProviders(<DashboardPage />);
    expect(await screen.findByRole("link", { name: /Ventes du jour/ })).toHaveAttribute(
      "href",
      "/reports/sales_summary?from=2026-09-29&to=2026-09-29",
    );
    expect(screen.getByRole("link", { name: /Hier/ })).toHaveAttribute(
      "href",
      "/reports/sales_summary?from=2026-09-28&to=2026-09-28",
    );
    expect(screen.getByText("Retours").closest("a")).toBeNull();
  });
});
