import type { CSSProperties } from "react";
import type { LabelSheet as Sheet, SheetBarcode } from "@/lib/ipc/types";

const mm = (dmm: number) => `${dmm / 10}mm`;
// A cell is placed by `inset-inline-start`/`top` (the classes `start-(--x)`, `top-(--y)`) from these
// custom properties, in millimetres from the sheet's start edge and its top.

const LATIN = "Inter, Cairo, system-ui, sans-serif";
const ARABIC = "Cairo, Inter, system-ui, sans-serif";

/** An Arabic sheet (its first label reads right to left) starts at the paper's right edge: its
 * position 1 is the top right label, and every cell is placed from the right. */
export function sheetRtl(sheet: Sheet): boolean {
  return sheet.labels[0]?.rtl ?? false;
}

/** The modules of a barcode as black runs: one rect per bar (a QR code: per run of a row). */
function Bars({ barcode }: { barcode: SheetBarcode }) {
  const width = barcode.rows[0]?.length ?? 0;
  const rects: { x: number; y: number; w: number }[] = [];
  barcode.rows.forEach((row, y) => {
    let x = 0;
    while (x < row.length) {
      if (row[x] === "1") {
        const start = x;
        while (x < row.length && row[x] === "1") x += 1;
        rects.push({ x: start, y, w: x - start });
      } else {
        x += 1;
      }
    }
  });
  const square = barcode.rows.length > 1;
  return (
    <svg
      viewBox={`0 0 ${width} ${barcode.rows.length}`}
      preserveAspectRatio={square ? "xMidYMid meet" : "none"}
      className={square ? "mx-auto h-[12mm]" : "h-[9mm] w-full"}
      aria-hidden
    >
      {rects.map((r) => (
        <rect key={`${r.x}-${r.y}`} x={r.x} y={r.y} width={r.w} height={1} fill="#000" />
      ))}
    </svg>
  );
}

/** LBL-013: one A4 planche per page, black on white whatever the theme; each label in its own
 * language and direction, numbers, prices and barcodes left to right. */
export function LabelSheet({ sheet }: { sheet: Sheet }) {
  const pages = Array.from({ length: sheet.pages }, (_, p) => sheet.cells.filter((c) => c.page === p));
  const f = sheet.format;
  const dir = sheetRtl(sheet) ? "rtl" : "ltr";
  return (
    <div className="label-sheet bg-white text-black tabular-nums">
      <style>
        {"@page { size: A4; margin: 0 } .label-sheet-page + .label-sheet-page { break-before: page }"}
      </style>
      {pages.map((cells, p) => (
        <section key={p} dir={dir} className="label-sheet-page relative h-[297mm] w-[210mm] overflow-hidden">
          {cells.map((cell, i) => {
            const label = sheet.labels[cell.label];
            if (!label) return null;
            // The positioned box keeps the sheet's direction, so `inset-inline-start` measures from
            // the sheet's start edge; the label's own direction and language only flow its text.
            return (
              <article
                key={`${cell.row}-${cell.column}`}
                data-testid={`sheet-cell-${i}`}
                dir={dir}
                className="absolute start-(--x) top-(--y) h-(--h) w-(--w) overflow-hidden"
                style={
                  {
                    "--x": mm(cell.xDmm),
                    "--y": mm(cell.yDmm),
                    "--w": mm(f.labelWDmm),
                    "--h": mm(f.labelHDmm),
                  } as CSSProperties
                }
              >
                <div
                  dir={label.rtl ? "rtl" : "ltr"}
                  lang={label.lang}
                  className="flex h-full flex-col justify-between p-[1.5mm] text-[2.8mm] leading-tight"
                  style={{ fontFamily: label.rtl ? ARABIC : LATIN }}
                >
                  {label.shop ? <p className="text-center text-[2.2mm]">{label.shop}</p> : null}
                  {label.name ? <p className="font-bold">{label.name}</p> : null}
                  {label.nameSecond ? <p className="text-[2.4mm]">{label.nameSecond}</p> : null}
                  {label.weight ? (
                    <p dir="ltr" className="text-[2.4mm]">
                      {label.weight}
                    </p>
                  ) : null}
                  {label.details.map((d) => (
                    <p key={d} className="text-[2.2mm]">
                      {d}
                    </p>
                  ))}
                  <p className="flex items-baseline justify-between gap-[1mm]">
                    {label.oldPrice ? (
                      <s dir="ltr" className="text-[2.4mm]">
                        {label.oldPrice}
                      </s>
                    ) : (
                      <span />
                    )}
                    {label.price ? (
                      <strong dir="ltr" className="text-[4.2mm]">
                        {label.price}
                      </strong>
                    ) : null}
                  </p>
                  {label.barcode ? (
                    <div>
                      <Bars barcode={label.barcode} />
                      {label.barcode.hri ? (
                        <p dir="ltr" className="text-center text-[2.2mm] tracking-[0.3mm]">
                          {label.barcode.hri}
                        </p>
                      ) : null}
                    </div>
                  ) : null}
                </div>
              </article>
            );
          })}
        </section>
      ))}
    </div>
  );
}
