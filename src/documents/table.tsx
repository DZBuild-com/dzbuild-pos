import type { ReactNode } from "react";
import { formatDate, formatNumber } from "@/lib/format";
import type { Cell, TableColumnPrint, TablePrint } from "@/lib/ipc/types";
import { docDay, docMoney, docQty, docRate } from "./format";
import { Ltr, PageNumbers } from "./parts";

const NUMERIC = new Set<TableColumnPrint["kind"]>(["money", "qty", "int", "percent"]);

/** One cell as printed, by its column's kind (the same kinds as the xlsx/csv writer). */
function cellText(m: TablePrint, column: TableColumnPrint, cell: Cell): ReactNode {
  if (cell === null) return null;
  const lang = m.lang;
  if (typeof cell === "number") {
    switch (column.kind) {
      case "money":
        return <Ltr>{docMoney(cell, lang)}</Ltr>;
      case "qty":
        return <Ltr>{docQty(cell, lang)}</Ltr>;
      case "percent":
        return <Ltr>{docRate(cell, lang)}</Ltr>;
      case "date_time":
        // an instant (ms): shown in Algiers time
        return <Ltr>{formatDate(cell, lang, { withTime: true })}</Ltr>;
      case "text":
      case "tag":
        return String(cell);
      default:
        return <Ltr>{formatNumber(cell, lang)}</Ltr>;
    }
  }
  switch (column.kind) {
    case "date":
      return <Ltr>{docDay(cell)}</Ltr>;
    case "date_time":
    case "code":
      return <Ltr>{cell}</Ltr>;
    case "tag":
      return column.tag ? (m.tagLabels[`${column.tag}.${cell}`] ?? cell) : cell;
    default:
      // a name or a label in either script keeps its own direction inside the page's
      return <bdi>{cell}</bdi>;
  }
}

/** Any list or report on paper (IMX-013): title, the shop and its NIF on top, the columns' labels
 * repeated on every page, the totals row. */
export function TableDocument({ m }: { m: TablePrint }) {
  const align = (column: TableColumnPrint) => (NUMERIC.has(column.kind) ? "doc-num" : undefined);
  return (
    <article className="doc" lang={m.lang} dir={m.rtl ? "rtl" : "ltr"} data-accent="ink">
      <PageNumbers label={m.pageLabel} />
      <header className="doc-head">
        <div>
          <h1 className="doc-title">{m.title}</h1>
          {m.subtitle ? <p>{m.subtitle}</p> : null}
        </div>
        <div>
          <p>
            <strong>{m.shop}</strong>
          </p>
          {m.nif ? (
            <p>
              {m.nifLabel} : <Ltr>{m.nif}</Ltr>
            </p>
          ) : null}
        </div>
      </header>
      <table className="doc-lines">
        <thead>
          <tr>
            {m.columns.map((column) => (
              <th key={column.key} className={align(column)}>
                {column.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {m.rows.map((row, i) => (
            <tr key={i}>
              {m.columns.map((column, j) => (
                <td key={column.key} className={align(column)}>
                  {cellText(m, column, row[j] ?? null)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
        {m.totals ? (
          <tfoot>
            <tr>
              {m.columns.map((column, j) => (
                <th key={column.key} className={align(column)}>
                  {j === 0 && (m.totals?.[0] ?? null) === null
                    ? m.totalLabel
                    : cellText(m, column, m.totals?.[j] ?? null)}
                </th>
              ))}
            </tr>
          </tfoot>
        ) : null}
      </table>
      <footer className="doc-foot">
        <span />
        <span>
          <Ltr>{m.printedAt}</Ltr>
        </span>
      </footer>
    </article>
  );
}
