import type { ContractPrint } from "@/lib/ipc/types";
import { docMoney, docQty } from "./format";
import { Ltr, PageNumbers, PartyCard, SignatureBoxes, type Words } from "./parts";

/** The instalment contract (TMP-001, TMP-009) in plain words: the parties, the goods, the price,
 * the down payment and what is financed, the schedule, the supporting documents given (never a
 * cheque), the clauses and both signatures. Every figure and word comes from the model. */
export function ContractPage({ model: m }: { model: ContractPrint }) {
  const lang = m.lang;
  const l = m.labels;
  const words: Words = { lang, labels: m.partyLabels, second: null };
  const money = (amount: number) => <Ltr>{docMoney(amount, lang)}</Ltr>;
  return (
    <article className="doc" lang={lang} dir={m.rtl ? "rtl" : "ltr"} data-accent="ink">
      <PageNumbers label={m.partyLabels.page} />
      <header className="doc-head">
        <div>
          <h1 className="doc-title">{m.title}</h1>
          <p>
            {m.partyLabels.number} : <Ltr>{m.number}</Ltr>
          </p>
          <p>
            {m.partyLabels.date} : <Ltr>{m.date}</Ltr>
          </p>
        </div>
      </header>
      <div className="doc-parties">
        <PartyCard m={words} p={m.seller} title={m.partyLabels.seller} />
        <PartyCard m={words} p={m.buyer} title={m.partyLabels.customer} />
      </div>
      <section>
        <h2 className="doc-legal">{l.items}</h2>
        <ul className="doc-items">
          {m.items.map((item, i) => (
            <li key={i} className="doc-row">
              <span>
                <bdi>{item.name}</bdi> × <Ltr>{docQty(item.qty, lang)}</Ltr>
              </span>
              {money(item.total)}
            </li>
          ))}
        </ul>
      </section>
      <section className="doc-totals">
        <div>
          <span>{l.total}</span>
          {money(m.total)}
        </div>
        <div>
          <span>{l.down}</span>
          {money(m.down)}
        </div>
        <div className="doc-due">
          <span>{l.financed}</span>
          {money(m.financed)}
        </div>
      </section>
      <h2 className="doc-legal">{l.schedule}</h2>
      <table className="doc-lines">
        <thead>
          <tr>
            <th className="doc-num">#</th>
            <th>{l.due}</th>
            <th className="doc-num">{l.amount}</th>
          </tr>
        </thead>
        <tbody>
          {m.dues.map(([day, amount], i) => (
            <tr key={day}>
              <td className="doc-num">
                <Ltr>{i + 1}</Ltr>
              </td>
              <td>
                <Ltr>{day}</Ltr>
              </td>
              <td className="doc-num">{money(amount)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {m.guarantees.length > 0 ? (
        <section>
          <h2 className="doc-legal">{l.guarantees}</h2>
          <ul className="doc-items">
            {m.guarantees.map((g, i) => (
              <li key={i}>
                <bdi>{g}</bdi>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      <ol className="doc-clauses">
        {m.clauses.map((clause) => (
          <li key={clause}>{clause}</li>
        ))}
      </ol>
      <SignatureBoxes start={m.signatures[0]} end={m.signatures[1]} />
    </article>
  );
}
