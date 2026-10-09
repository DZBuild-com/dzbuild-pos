import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { LabelSheet } from "@/documents/label-sheet";

const format = {
  id: "a4_24_70x37",
  columns: 3,
  rows: 8,
  labelWDmm: 700,
  labelHDmm: 370,
  marginTopDmm: 5,
  marginStartDmm: 0,
  gapXDmm: 0,
  gapYDmm: 0,
};

const arabic = {
  rtl: true,
  lang: "ar" as const,
  shop: null,
  name: "حليب كانديا",
  nameSecond: "Lait Candia 1 L",
  price: "150,00 دج",
  oldPrice: null,
  weight: null,
  details: [],
  barcode: null,
};

const french = {
  ...arabic,
  rtl: false,
  lang: "fr" as const,
  shop: "Supérette El Amel",
  name: "Café Bonal 250 g",
  nameSecond: null,
  price: "320,00 DA",
};

/** Two labels in the same row: cell 0 at the sheet's start edge, cell 1 one label further. */
function mixed(first: typeof arabic | typeof french, second: typeof arabic | typeof french) {
  return {
    format,
    pages: 1,
    cells: [
      { page: 0, row: 0, column: 0, xDmm: 0, yDmm: 5, label: 0 },
      { page: 0, row: 0, column: 1, xDmm: 700, yDmm: 5, label: 1 },
    ],
    labels: [first, second],
  };
}

describe("LabelSheet cell direction", () => {
  it("places a French label of an Arabic sheet from the sheet's right edge and only flows its text left to right", () => {
    render(<LabelSheet sheet={mixed(arabic, french)} />);
    const box = screen.getByTestId("sheet-cell-1");
    // The box that carries inset-inline-start resolves its direction from the sheet, never the label.
    expect(box).toHaveClass("start-(--x)", "top-(--y)");
    expect(box.closest("[dir]")).toHaveAttribute("dir", "rtl");
    const text = screen.getByText("Café Bonal 250 g").closest("[dir]");
    expect(text).not.toBe(box);
    expect(box).toContainElement(text as HTMLElement);
    expect(text).toHaveAttribute("dir", "ltr");
    expect(text).toHaveAttribute("lang", "fr");
    expect(text).toHaveTextContent("Supérette El Amel");
    expect(text).not.toHaveClass("start-(--x)");
    expect((text as HTMLElement).style.fontFamily).toMatch(/^Inter/);
  });

  it("places an Arabic label of a French sheet from the sheet's left edge and only flows its text right to left", () => {
    render(<LabelSheet sheet={mixed(french, arabic)} />);
    const box = screen.getByTestId("sheet-cell-1");
    expect(box.closest("[dir]")).toHaveAttribute("dir", "ltr");
    const text = screen.getByText("حليب كانديا").closest("[dir]");
    expect(text).not.toBe(box);
    expect(box).toContainElement(text as HTMLElement);
    expect(text).toHaveAttribute("dir", "rtl");
    expect(text).toHaveAttribute("lang", "ar");
    expect((text as HTMLElement).style.fontFamily).toMatch(/^Cairo/);
  });
});
