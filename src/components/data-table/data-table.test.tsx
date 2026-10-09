import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { ExportMenu } from "@/components/data-table/export-menu";
import { ReportTable } from "@/components/data-table/report-table";
import { customerQuery } from "@/lib/ipc/customers";
import { mockCommands, providers, renderWithProviders } from "@/test/render";
import { reportViewFixture } from "@/test/fixtures-m2";

type Row = { id: string; name: string; qty: number };
const col = columnHelper<Row>();
const columns = col.columns([
  col.accessor("name", { header: "Nom" }),
  col.accessor("qty", { header: "Qté", enableSorting: true }),
]);
const rows: Row[] = Array.from({ length: 10_000 }, (_, i) => ({
  id: `p${i}`,
  name: `Article ${i}`,
  qty: (i * 7919) % 1000,
}));

beforeEach(() => {
  // jsdom has no layout: TanStack Virtual measures its scroller with offsetWidth/offsetHeight and
  // shows no row at 0×0, so every element measures 1024×600.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

describe("DataTable", () => {
  it("keeps only the visible rows in the DOM and sorts from the header", () => {
    render(<DataTable ariaLabel="Articles" data={rows} columns={columns} getRowId={(r) => r.id} />, {
      wrapper: providers(),
    });
    const grid = screen.getByRole("table", { name: "Articles" });
    const shown = within(grid).getAllByRole("row").length;
    expect(shown).toBeGreaterThan(1);
    expect(shown).toBeLessThan(80);
    const header = screen.getByRole("columnheader", { name: /Qté/ });
    fireEvent.click(within(header).getByRole("button"));
    expect(header).toHaveAttribute("aria-sort", "ascending");
    const first = within(grid).getAllByRole("row")[1]!;
    expect(within(first).getAllByRole("cell")[1]).toHaveTextContent(/^0$/);
  });

  it("moves between rows with the arrow keys and opens one with Enter", () => {
    const opened: Row[] = [];
    render(
      <DataTable
        ariaLabel="Articles"
        data={rows}
        columns={columns}
        getRowId={(r) => r.id}
        onRowClick={(r) => opened.push(r)}
      />,
      { wrapper: providers() },
    );
    const [, first] = within(screen.getByRole("table", { name: "Articles" })).getAllByRole("row");
    expect(first).toHaveAttribute("tabindex", "0");
    first!.focus();
    fireEvent.keyDown(first!, { key: "ArrowDown" });
    expect(document.activeElement).toHaveTextContent("Article 1");
    fireEvent.keyDown(document.activeElement!, { key: "ArrowUp" });
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(first!, { key: "Enter" });
    expect(opened).toEqual([rows[0]]);
  });

  it("formats a backend report by column kind and shows its totals", () => {
    // The UI language is French in every test (src/test/setup.ts).
    render(<ReportTable view={reportViewFixture} />, { wrapper: providers() });
    expect(screen.getByRole("columnheader", { name: "Net TTC" })).toBeInTheDocument();
    expect(screen.getByText("Espèces")).toBeInTheDocument();
    expect(screen.getAllByText(/1\s?190,00/).length).toBeGreaterThan(0);
    expect(screen.getByRole("row", { name: /Total/ })).toBeInTheDocument();
  });

  it("exports the table to PDF through the backend", async () => {
    const seen: unknown[] = [];
    mockCommands({
      exports_pdf: (args) => {
        seen.push(args);
        return { path: "C:\\docs\\ventes.pdf", rows: 3 };
      },
    });
    renderWithProviders(<ExportMenu target={{ kind: "customers", query: customerQuery({}) }} />);
    // M2's export menu is a row of buttons (Excel, CSV, PDF), not a drop-down menu.
    await userEvent.click(screen.getByRole("button", { name: "PDF" }));
    await waitFor(() =>
      expect(seen[0]).toMatchObject({ request: { target: { kind: "customers" }, lang: "fr" } }),
    );
  });
});
