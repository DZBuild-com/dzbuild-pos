import type { ZakatStatementPrint } from "@/lib/ipc/types";
import { docMoney } from "./format";
import { Ltr, PageNumbers, PartyCard, type Words } from "./parts";

/** The indicative zakat statement (FIN-007): the shop, both dates, the seven figures of the base
 * and the base itself, the nisab with its source, the rate, the zakat due in large type (or the
 * "below the nisab" sentence), and the "not a fatwa" disclaimer in a box. Every word and figure
 * comes from the model; the page computes nothing. */
export function ZakatDocument({ m }: { m: ZakatStatementPrint }) {
  const lang = m.lang;
  const words: Words = { lang, labels: m.partyLabels, second: null };
  const money = (amount: number) => <Ltr>{docMoney(amount, lang)}</Ltr>;
  return (
    <article className="doc" lang={lang} dir={m.rtl ? "rtl" : "ltr"} data-accent="ink">
      <PageNumbers label={m.partyLabels.page} />
      <header className="doc-head">
        <div>
          <h1 className="doc-title">{m.title}</h1>
          <p>
            {m.partyLabels.number} <Ltr>{m.number}</Ltr>
          </p>
          <p>
            {m.partyLabels.date} <Ltr>{m.date}</Ltr> — <bdi>{m.hijri}</bdi>
          </p>
        </div>
      </header>
      <div className="doc-parties">
        <PartyCard m={words} p={m.seller} />
      </div>
      <table className="doc-lines">
        <tbody>
          {m.lines.map(([label, amount]) => (
            <tr key={label}>
              <th scope="row">
                <bdi>{label}</bdi>
              </th>
              <td className="doc-num">{money(amount)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="doc-totals">
        <div>
          <strong>{m.labels.base}</strong>
          <strong>{money(m.base)}</strong>
        </div>
        <div>
          <span>{m.labels.nisab}</span>
          {money(m.nisab)}
        </div>
        <div>
          <span>{m.labels.rate}</span>
          <Ltr>{m.rate}</Ltr>
        </div>
        {m.belowNisab === null ? (
          <div className="doc-due">
            <span>{m.labels.due}</span>
            {money(m.due)}
          </div>
        ) : null}
      </div>
      {m.belowNisab !== null ? <p className="doc-legal">{m.belowNisab}</p> : null}
      <p className="doc-legal">{m.source}</p>
      <p className="doc-disclaimer">{m.disclaimer}</p>
    </article>
  );
}
