import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { LabelSheet } from "@/documents/label-sheet";
import { DocumentPage, pageDirection } from "@/documents/page";

const sheet = {
  format: {
    id: "a4_24_70x37",
    columns: 3,
    rows: 8,
    labelWDmm: 700,
    labelHDmm: 370,
    marginTopDmm: 5,
    marginStartDmm: 0,
    gapXDmm: 0,
    gapYDmm: 0,
  },
  pages: 1,
  cells: [{ page: 0, row: 7, column: 1, xDmm: 700, yDmm: 2595, label: 0 }],
  labels: [
    {
      rtl: true,
      lang: "ar" as const,
      shop: null,
      name: "حليب كانديا",
      nameSecond: "Lait Candia 1 L",
      price: "150,00 دج",
      oldPrice: null,
      weight: null,
      details: [],
      barcode: { symbology: "ean13" as const, data: "6130123456782", rows: ["1010"], hri: "6130123456782" },
    },
  ],
};

describe("LabelSheet", () => {
  it("places each cell in millimetres from the start edge and draws the bars", () => {
    render(<LabelSheet sheet={sheet} />);
    const cell = screen.getByTestId("sheet-cell-0");
    // inset-inline-start: var(--x); top: var(--y); width: var(--w); height: var(--h)
    expect(cell).toHaveClass("start-(--x)", "top-(--y)", "w-(--w)", "h-(--h)");
    expect(["--x", "--y", "--w", "--h"].map((p) => cell.style.getPropertyValue(p))).toEqual([
      "70mm",
      "259.5mm",
      "70mm",
      "37mm",
    ]);
    expect(cell).toHaveAttribute("dir", "rtl");
    expect(cell.querySelectorAll("svg rect")).toHaveLength(2);
    expect(screen.getByText("6130123456782")).toHaveAttribute("dir", "ltr");
  });

  it("lays an Arabic sheet out from its right edge, one A4 page per planche", () => {
    const two = {
      ...sheet,
      pages: 2,
      cells: [...sheet.cells, { ...sheet.cells[0]!, page: 1, row: 0, yDmm: 5 }],
    };
    const { container } = render(<LabelSheet sheet={two} />);
    const pages = container.querySelectorAll("section");
    expect(pages).toHaveLength(2);
    expect(pages[0]).toHaveAttribute("dir", "rtl");
    expect(pages[1]!.querySelectorAll("article")).toHaveLength(1);
  });

  it("is the print host's label_sheet page, right to left when its labels are Arabic", () => {
    const page = { template: "label_sheet" as const, model: sheet };
    render(<DocumentPage page={page} />);
    expect(screen.getByTestId("sheet-cell-0")).toHaveTextContent("حليب كانديا");
    expect(
      pageDirection({ jobId: "j1", lang: "ar", page, marginMm: 0, sizeMm: [210, 297], demoMark: null }),
    ).toEqual({
      lang: "ar",
      dir: "rtl",
    });
  });
});
