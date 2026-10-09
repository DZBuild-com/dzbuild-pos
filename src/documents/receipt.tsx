import type { ReactNode } from "react";
import type { ReceiptLabels, ReceiptPage } from "@/lib/ipc/types";
import { docDay, docMoney, docQty, docRate } from "./format";
import { Ltr } from "./parts";

type Key = keyof ReceiptLabels;

/** The PDF (or A4 copy) of a ticket or a return note (SAL-052): the thermal receipt's content in an
 * 80 mm column, every word from the model, the second language paired under the first. */
export function ReceiptDocument({ page }: { page: ReceiptPage }) {
  const r = page.receipt;
  const lang = r.lang;
  const second = page.second;
  const word = (k: Key) => (
    <span>
      {page.labels[k]}
      {second ? <span className="doc-alt">{second[k]}</span> : null}
    </span>
  );
  const money = (amount: number) => <Ltr>{docMoney(amount, lang)}</Ltr>;
  const row = (label: ReactNode, value: ReactNode, key?: string) => (
    <div className="doc-row" key={key}>
      {label}
      {value}
    </div>
  );
  const h = r.header;
  const ids: Array<[Key, string | null]> = [
    ["rc", h.rc],
    ["nif", h.nif],
    ["nis", h.nis],
    ["ai", h.ai],
  ];
  return (
    <article className="doc doc-receipt" lang={lang} dir={r.rtl ? "rtl" : "ltr"}>
      <header className="doc-receipt-head">
        <p>
          <strong>{h.name}</strong>
        </p>
        {h.legalForm ? <p dir="auto">{h.legalForm}</p> : null}
        {h.address ? <p dir="auto">{h.address}</p> : null}
        {h.phone ? (
          <p>
            {word("phone")} <Ltr>{h.phone}</Ltr>
          </p>
        ) : null}
        {ids.map(([k, value]) =>
          value ? (
            <p key={k}>
              {word(k)} <Ltr>{value}</Ltr>
            </p>
          ) : null,
        )}
        {h.headerText ? <p>{h.headerText}</p> : null}
      </header>
      <h1 className="doc-title">{word("title")}</h1>
      {r.duplicate ? <p className="doc-copy">{word("duplicate")}</p> : null}
      {row(word("number"), <Ltr>{r.number}</Ltr>)}
      {row(word("date"), <Ltr>{r.localTime}</Ltr>)}
      {row(word("terminal"), <Ltr>{r.terminalCode}</Ltr>)}
      {row(word("cashier"), <span>{r.cashier}</span>)}
      {r.customer ? row(word("customer"), <span>{r.customer.name}</span>) : null}
      {r.customer ? row(word("customerCode"), <Ltr>{r.customer.code}</Ltr>) : null}
      {r.originalNumber ? row(word("original"), <Ltr>{r.originalNumber}</Ltr>) : null}
      <section className="doc-receipt-lines">
        {r.lines.map((line, i) => (
          <div key={i}>
            <p>
              <bdi>{line.description}</bdi>
            </p>
            {row(
              <Ltr>
                {docQty(line.qty, lang)}
                {line.unitCode === "pc" ? "" : ` ${line.unitCode}`} × {docMoney(line.unitPrice, lang)}
              </Ltr>,
              money(line.total),
            )}
            {line.discount ? row(word("discount"), money(-line.discount)) : null}
            {line.components.map((c, j) => (
              <p key={j} className="doc-alt">
                <bdi>{c.description}</bdi> × <Ltr>{docQty(c.qty, lang)}</Ltr>
              </p>
            ))}
            {line.note ? <p className="doc-alt">{line.note}</p> : null}
          </div>
        ))}
      </section>
      {row(word("items"), <Ltr>{docQty(r.itemCount, lang)}</Ltr>)}
      {r.showTax ? row(word("subtotalHt"), money(r.subtotalHt)) : null}
      {r.showTax
        ? r.taxes.map((tax) =>
            row(
              <span>
                {word("vat")} <Ltr>{docRate(tax.rateBp, lang)}</Ltr> (<Ltr>{docMoney(tax.baseHt, lang)}</Ltr>)
              </span>,
              money(tax.tax),
              String(tax.rateBp),
            ),
          )
        : null}
      {row(word("totalTtc"), money(r.totalTtc))}
      {r.stampDuty ? row(word("stampDuty"), money(r.stampDuty)) : null}
      {r.rounding ? row(word("rounding"), money(r.rounding)) : null}
      <div className="doc-due">
        {word(r.docType === "return_note" ? "refund" : "netToPay")}
        {money(r.totalDue)}
      </div>
      {r.stampAbsorbed ? (
        <p className="doc-alt">
          {word("stampAbsorbed")} : {money(r.stampAbsorbed)}
        </p>
      ) : null}
      <section className="doc-receipt-payments">
        {r.payments.map((p, i) => (
          <div key={i}>
            {row(
              <span>
                {page.methods[i] ?? ""}
                {page.secondMethods?.[i] ? <span className="doc-alt">{page.secondMethods[i]}</span> : null}
              </span>,
              money(p.amount),
            )}
            {p.reference ? (
              <p className="doc-alt">
                <Ltr>{p.reference}</Ltr>
              </p>
            ) : null}
            {p.chequeBank ? (
              <p className="doc-alt">
                {word("chequeBank")} {p.chequeBank}
              </p>
            ) : null}
            {p.chequeDate ? (
              <p className="doc-alt">
                {word("chequeDate")} <Ltr>{docDay(p.chequeDate)}</Ltr>
              </p>
            ) : null}
          </div>
        ))}
      </section>
      {r.change ? row(word("change"), money(r.change)) : null}
      {r.customer && r.customerBalance !== null
        ? row(word("customerBalance"), money(r.customerBalance))
        : null}
      {r.discountTotal > 0 ? row(word("youSaved"), money(r.discountTotal)) : null}
      {r.totalDue ? (
        <p className="doc-letters">
          {word("inLetters")} : {r.totalInLetters}
        </p>
      ) : null}
      {r.ifuMention ? <p className="doc-legal">{word("ifuMention")}</p> : null}
      {r.note ? <p>{r.note}</p> : null}
      {r.footerText ? <p dir="auto">{r.footerText}</p> : null}
      <p className="doc-legal">{word("thanks")}</p>
      <p className="doc-receipt-code">
        <Ltr>{r.barcode}</Ltr>
      </p>
    </article>
  );
}
