import type { ReactNode } from "react";
import type { Lang } from "@/lib/i18n";
import type { DocumentLabels, DocumentPrintModel, PartyBlock, QrMatrix } from "@/lib/ipc/types";
import { docDay, docMoney, docQty, docRate } from "./format";

type LabelKey = keyof DocumentLabels;

/** The second language of a bilingual page. */
export interface Second {
  lang: Lang;
  rtl: boolean;
  labels: DocumentLabels;
}

/** The words a card or a label is printed with: the page's language and, when bilingual, the second. */
export interface Words {
  lang: Lang;
  labels: DocumentLabels;
  second: Second | null;
}

/** Numbers, identifiers, amounts and dates stay left to right inside an Arabic line. */
export const Ltr = ({ children }: { children: ReactNode }) => <bdi dir="ltr">{children}</bdi>;

/** A text and, on a bilingual document, the same text in the second language: under it, or on the
 * same line after a slash (`inline`, for a label followed by its value). */
export function Twin({
  text,
  alt,
  second,
  inline = false,
}: {
  text: ReactNode;
  alt?: string | null;
  second?: Second | null;
  inline?: boolean;
}) {
  return (
    <span>
      {text}
      {alt && second ? (
        <>
          {inline ? " / " : null}
          <span
            className={inline ? "doc-twin" : "doc-alt"}
            lang={second.lang}
            dir={second.rtl ? "rtl" : "ltr"}
          >
            {alt}
          </span>
        </>
      ) : null}
    </span>
  );
}

/** A label and, on a bilingual document, its twin in the second language. */
export function Pair({ m, k, inline = false }: { m: Words; k: LabelKey; inline?: boolean }) {
  return <Twin text={m.labels[k]} alt={m.second?.labels[k]} second={m.second} inline={inline} />;
}

const ID_LABEL: Record<PartyBlock["ids"][number]["kind"], LabelKey> = {
  rc: "rc",
  ae_card: "aeCard",
  nif: "nif",
  nis: "nis",
  ai: "ai",
  nin: "nin",
  rib: "rib",
};

/** A party with its legal mentions (décret 05-468 art. 3): name, form, capital, activity, address,
 * phone and its identifiers in the order the model gives (RC or AE card, NIF, NIS, AI, NIN, RIB). */
export function PartyCard({ m, p, title }: { m: Words; p: PartyBlock; title?: ReactNode }) {
  return (
    <section className="doc-party">
      {title ? <h2 className="doc-legal">{title}</h2> : null}
      <p>
        <strong>
          <bdi>{p.name}</bdi>
        </strong>
        {p.nameAlt && p.nameAlt !== p.name ? (
          <span className="doc-alt" dir="auto">
            {p.nameAlt}
          </span>
        ) : null}
      </p>
      {p.legalForm ? (
        <p>
          <Pair m={m} k="legalForm" inline /> : <bdi>{p.legalForm}</bdi>
          {p.capital !== null ? (
            <>
              {" — "}
              <Pair m={m} k="capital" inline /> : <Ltr>{docMoney(p.capital, m.lang)}</Ltr>
            </>
          ) : null}
        </p>
      ) : null}
      {p.activity ? (
        <p>
          <Pair m={m} k="activity" inline /> : <bdi>{p.activity}</bdi>
          {p.activityAlt && p.activityAlt !== p.activity ? (
            <span className="doc-alt" dir="auto">
              {p.activityAlt}
            </span>
          ) : null}
        </p>
      ) : null}
      {p.address ? (
        <p>
          <Pair m={m} k="address" inline /> : <bdi>{p.address}</bdi>
          {p.wilaya ? (
            <>
              {" — "}
              <bdi>{p.wilaya}</bdi>
            </>
          ) : null}
        </p>
      ) : null}
      {p.phone ? (
        <p>
          <Pair m={m} k="phone" inline /> : <Ltr>{p.phone}</Ltr>
        </p>
      ) : null}
      {p.email ? (
        <p>
          <Pair m={m} k="email" inline /> : <Ltr>{p.email}</Ltr>
        </p>
      ) : null}
      <div className="doc-ids">
        {p.ids.map((id) => (
          <span key={id.kind} className="doc-id">
            <Pair m={m} k={ID_LABEL[id.kind]} />
            <Ltr>{id.value}</Ltr>
          </span>
        ))}
      </div>
    </section>
  );
}

/** The dark modules of a QR matrix as one SVG path, a rectangle per run of dark modules in a row
 * (one element to lay out instead of hundreds). */
export function qrPath(qr: QrMatrix): string {
  const runs: string[] = [];
  for (let y = 0; y < qr.size; y += 1) {
    let x = 0;
    while (x < qr.size) {
      if (qr.modules[y * qr.size + x] !== "1") {
        x += 1;
        continue;
      }
      const start = x;
      while (x < qr.size && qr.modules[y * qr.size + x] === "1") x += 1;
      runs.push(`M${start} ${y}h${x - start}v1h-${x - start}z`);
    }
  }
  return runs.join("");
}

/** The QR of the document number, drawn from its module matrix (no network, no image). */
export function QrCode({ qr, label }: { qr: QrMatrix; label: string }) {
  return (
    <svg
      className="doc-qr"
      viewBox={`0 0 ${qr.size} ${qr.size}`}
      role="img"
      aria-label={label}
      fill="currentColor"
      shapeRendering="crispEdges"
    >
      <path d={qrPath(qr)} />
    </svg>
  );
}

/** The lines: HT prices on every document of a régime-réel shop (décret 05-468 art. 3, an exempt
 * buyer's too: `priceBasisHt`), the TVA column only when TVA is charged (`showTax`). */
export function LinesTable({ m }: { m: DocumentPrintModel }) {
  const lang = m.lang;
  const price = m.priceBasisHt ? "unitPriceHt" : "unitPriceTtc";
  const total = m.priceBasisHt ? "totalHt" : "totalTtc";
  return (
    <table className="doc-lines">
      <thead>
        <tr>
          <th className="doc-num">#</th>
          <th>
            <Pair m={m} k="designation" />
          </th>
          <th className="doc-num">
            <Pair m={m} k="qty" />
          </th>
          <th>
            <Pair m={m} k="unit" />
          </th>
          {m.showPrices ? (
            <th className="doc-num">
              <Pair m={m} k={price} />
            </th>
          ) : null}
          {m.showPrices ? (
            <th className="doc-num">
              <Pair m={m} k="discount" />
            </th>
          ) : null}
          {m.showPrices && m.showTax ? (
            <th className="doc-num">
              <Pair m={m} k="vatRate" />
            </th>
          ) : null}
          {m.showPrices ? (
            <th className="doc-num">
              <Pair m={m} k={total} />
            </th>
          ) : null}
        </tr>
      </thead>
      <tbody>
        {m.lines.map((l) => (
          <tr key={l.no}>
            <td className="doc-num">
              <Ltr>{l.no}</Ltr>
            </td>
            <td>
              <bdi>{l.description}</bdi>
              {l.descriptionAlt && m.second ? (
                <span className="doc-alt" lang={m.second.lang} dir={m.second.rtl ? "rtl" : "ltr"}>
                  {l.descriptionAlt}
                </span>
              ) : null}
              {l.components.map((c, i) => (
                <span key={i} className="doc-alt">
                  <bdi>{c.description}</bdi> × <Ltr>{docQty(c.qty, lang)}</Ltr>
                </span>
              ))}
              {l.note ? <span className="doc-alt">{l.note}</span> : null}
            </td>
            <td className="doc-num">
              <Ltr>{docQty(l.qty, lang)}</Ltr>
            </td>
            <td>{l.unit}</td>
            {m.showPrices ? (
              <td className="doc-num">
                <Ltr>{docMoney(m.priceBasisHt ? l.unitPriceHt : l.unitPriceTtc, lang)}</Ltr>
              </td>
            ) : null}
            {m.showPrices ? (
              <td className="doc-num">{l.discount ? <Ltr>{docMoney(l.discount, lang)}</Ltr> : null}</td>
            ) : null}
            {m.showPrices && m.showTax ? (
              <td className="doc-num">
                <Ltr>{docRate(l.rateBp, lang)}</Ltr>
              </td>
            ) : null}
            {m.showPrices ? (
              <td className="doc-num">
                <Ltr>{docMoney(m.priceBasisHt ? l.totalHt : l.totalTtc, lang)}</Ltr>
              </td>
            ) : null}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** HT (régime réel, an exempt buyer's document too), discounts, TVA per rate (when charged), TTC,
 * the stamp duty on its own line and the "Net à payer" band: every figure from the model, none
 * computed here. */
export function TotalsBlock({ m }: { m: DocumentPrintModel }) {
  const lang = m.lang;
  const t = m.totals;
  const row = (k: LabelKey, amount: number) => (
    <div key={k}>
      <Pair m={m} k={k} />
      <Ltr>{docMoney(amount, lang)}</Ltr>
    </div>
  );
  return (
    <section className="doc-totals">
      {m.priceBasisHt ? row("subtotalHt", t.subtotalHt) : null}
      {t.discountTotal ? row("discountTotal", t.discountTotal) : null}
      {m.showTax
        ? t.taxes.map((tax, i) => (
            <div key={tax.rateBp}>
              <Twin text={tax.label} alt={m.second?.taxLabels[i]} second={m.second} />
              <Ltr>{docMoney(tax.tax, lang)}</Ltr>
            </div>
          ))
        : null}
      {row("totalTtcLabel", t.totalTtc)}
      {t.stampDuty ? row("stampDuty", t.stampDuty) : null}
      {t.rounding ? row("rounding", t.rounding) : null}
      <div className="doc-due">
        <Pair m={m} k="netToPay" />
        <Ltr>{docMoney(t.totalDue, lang)}</Ltr>
      </div>
      {t.stampAbsorbed ? (
        <p className="doc-alt">
          <Pair m={m} k="stampAbsorbed" inline /> <Ltr>{docMoney(t.stampAbsorbed, lang)}</Ltr>
        </p>
      ) : null}
      {t.remaining ? row("remaining", t.remaining) : null}
    </section>
  );
}

/** The tenders of an invoice or a credit note, with a cheque's number, bank and date (PAY-023). */
export function PaymentBlock({ m }: { m: DocumentPrintModel }) {
  if (!m.payments.length) return null;
  return (
    <section className="doc-payments">
      {m.payments.map((p, i) => {
        const details = [p.reference, p.chequeBank, p.chequeDate ? docDay(p.chequeDate) : null].filter(
          Boolean,
        );
        return (
          <div key={i}>
            <span>
              {p.label}
              {details.length ? (
                <span className="doc-alt">
                  <Ltr>{details.join(" — ")}</Ltr>
                </span>
              ) : null}
            </span>
            <Ltr>{docMoney(p.amount, m.lang)}</Ltr>
          </div>
        );
      })}
    </section>
  );
}

/** Whether the document carries TVA: the exemption line is then never printed. */
const carriesVat = (m: DocumentPrintModel) =>
  m.showTax ||
  m.totals.taxTotal !== 0 ||
  m.totals.taxes.some((t) => t.tax !== 0) ||
  m.lines.some((l) => l.rateBp > 0);

export function LegalBlock({ m }: { m: DocumentPrintModel }) {
  return (
    <section>
      {m.ifuMention ? <p className="doc-legal">{m.ifuMention}</p> : null}
      {m.exemptMention && !carriesVat(m) ? <p className="doc-legal">{m.exemptMention}</p> : null}
      {m.legal.map((text, i) => (
        <p key={text} className="doc-legal">
          <Twin text={text} alt={m.second?.legal[i]} second={m.second} />
        </p>
      ))}
      {m.inLetters ? (
        <p className="doc-letters">
          <Twin text={m.inLetters} alt={m.second?.inLetters} second={m.second} />
        </p>
      ) : null}
      {m.paymentMode ? (
        <p>
          <Pair m={m} k="paymentMode" inline /> :{" "}
          <Twin text={m.paymentMode} alt={m.second?.paymentMode} second={m.second} />
        </p>
      ) : null}
      {m.references.length ? (
        <p>
          <Pair m={m} k="references" inline /> :{" "}
          {m.references.map((r) => (
            <span key={r.number}>
              {r.label} <Ltr>{r.number}</Ltr> (<Ltr>{r.date}</Ltr>){" "}
            </span>
          ))}
        </p>
      ) : null}
    </section>
  );
}

export function Head({ m }: { m: DocumentPrintModel }) {
  return (
    <header className="doc-head">
      <div>
        {m.logo && m.layout.showLogo ? <img className="doc-logo" src={m.logo} alt="" /> : null}
        {m.headerText ? <p>{m.headerText}</p> : null}
      </div>
      <div>
        <h1 className="doc-title">
          <Twin text={m.title} alt={m.second?.title} second={m.second} />
        </h1>
        {m.duplicate ? <p className="doc-copy">{m.duplicate}</p> : null}
        <p>
          <Pair m={m} k="number" inline /> : <Ltr>{m.number ?? ""}</Ltr>
        </p>
        <p>
          <Pair m={m} k="date" inline /> : <Ltr>{m.date}</Ltr>
        </p>
        {m.validUntil ? (
          <p>
            <Pair m={m} k="validUntil" inline /> : <Ltr>{m.validUntil}</Ltr>
          </p>
        ) : null}
        {m.dueDate ? (
          <p>
            <Pair m={m} k="dueDate" inline /> : <Ltr>{m.dueDate}</Ltr>
          </p>
        ) : null}
      </div>
      {m.qr ? <QrCode qr={m.qr} label={m.number ?? ""} /> : null}
    </header>
  );
}

/** A CSS string: quotes and backslashes escaped, no line break, no way out of the `<style>`. */
const cssString = (text: string) =>
  `"${text
    .replace(/[\\"]/g, "\\$&")
    .replace(/[\r\n]+/g, " ")
    .replace(/</g, "\\3C ")}"`;

/** "Page 1 / 2" at the bottom of every sheet, in the document's words: page counters exist only
 * in the `@page` margin boxes, so the word travels in the rule (styled in documents.css). */
export function PageNumbers({ label }: { label: string }) {
  return (
    <style>{`@page{@bottom-center{content:${cssString(`${label} `)} counter(page) " / " counter(pages)}}`}</style>
  );
}

/** Footer texts, social links and the print time; the page numbers are in the sheet's margin. */
export function Footer({ m }: { m: DocumentPrintModel }) {
  return (
    <footer className="doc-foot">
      <span>{[m.footerText, m.termsText, ...m.social].filter(Boolean).join(" — ")}</span>
      <span>
        <Pair m={m} k="printedAt" inline /> <Ltr>{m.printedAt}</Ltr>
      </span>
    </footer>
  );
}

/** Two signature boxes, 40 mm high, side by side: the first caption at the page's start side, the
 * second at its end side (the seller and the buyer of a contract, the driver and the shop of a
 * route sheet). */
export function SignatureBoxes({ start, end }: { start: string; end: string }) {
  return (
    <div className="doc-signatures">
      <div className="doc-signature">
        <p>{start}</p>
      </div>
      <div className="doc-signature">
        <p>{end}</p>
      </div>
    </div>
  );
}

/** "Cachet et signature" (décret 05-468 art. 4): always on FA and AV, on the others by setting. */
export function Stamp({ m }: { m: DocumentPrintModel }) {
  return m.stampBox ? (
    <div className="doc-stamp">
      <Pair m={m} k="stampBox" />
    </div>
  ) : null;
}

/** The diagonal mark: cancelled by credit notes (DOC-007), draft, expired offer. */
export function Watermark({ m }: { m: DocumentPrintModel }) {
  return m.watermark ? (
    <div className={`doc-watermark doc-watermark-${m.watermark.kind}`}>{m.watermark.text}</div>
  ) : null;
}

/** The sheet of every commercial document: direction and language from the model, the accent of
 * the settings, the head, the body and the footer. */
export function Frame({ m, children }: { m: DocumentPrintModel; children: ReactNode }) {
  return (
    <article className="doc" lang={m.lang} dir={m.rtl ? "rtl" : "ltr"} data-accent={m.layout.accent}>
      <PageNumbers label={m.labels.page} />
      <Watermark m={m} />
      {m.duplicate ? (
        <p className="doc-copy-mark" aria-hidden="true">
          {m.duplicate}
        </p>
      ) : null}
      <Head m={m} />
      {children}
      <Footer m={m} />
    </article>
  );
}
