import type { StatementA4 } from "@/lib/ipc/types";
import { docMoney } from "./format";
import { Ltr, PageNumbers, PartyCard, type Words } from "./parts";

/** The A4 statement of a customer's or a supplier's account (CRD-005, PUR-006): debits and credits
 * in two columns, the running balance, opening and closing. */
export function StatementDocument({ m }: { m: StatementA4 }) {
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
            {l.period} : <Ltr>{m.period}</Ltr>
          </p>
        </div>
      </header>
      <div className="doc-parties">
        <PartyCard m={words} p={m.seller} title={m.partyLabels.seller} />
        <section className="doc-party">
          <h2 className="doc-legal">{m.party.isSupplier ? l.supplier : l.customer}</h2>
          <p>
            <strong>
              <bdi>{m.party.name}</bdi>
            </strong>
          </p>
          <p>
            {l.code} : <Ltr>{m.party.code}</Ltr>
          </p>
          {m.party.address ? (
            <p>
              {l.address} : <bdi>{m.party.address}</bdi>
            </p>
          ) : null}
          <div className="doc-ids">
            {m.party.rc ? (
              <span className="doc-id">
                <span>{l.rc}</span>
                <Ltr>{m.party.rc}</Ltr>
              </span>
            ) : null}
            {m.party.nif ? (
              <span className="doc-id">
                <span>{l.nif}</span>
                <Ltr>{m.party.nif}</Ltr>
              </span>
            ) : null}
          </div>
        </section>
      </div>
      <table className="doc-lines">
        <thead>
          <tr>
            <th>{l.date}</th>
            <th>{l.label}</th>
            <th>{l.number}</th>
            <th className="doc-num">{l.debit}</th>
            <th className="doc-num">{l.credit}</th>
            <th className="doc-num">{l.balance}</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td colSpan={5}>{l.opening}</td>
            <td className="doc-num">{money(m.opening)}</td>
          </tr>
          {m.lines.map((line, i) => (
            <tr key={i}>
              <td>
                <Ltr>{line.date}</Ltr>
              </td>
              <td>{line.label}</td>
              <td>{line.number ? <Ltr>{line.number}</Ltr> : null}</td>
              <td className="doc-num">{line.debit ? money(line.debit) : null}</td>
              <td className="doc-num">{line.credit ? money(line.credit) : null}</td>
              <td className="doc-num">{money(line.balance)}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <th colSpan={3}>{l.total}</th>
            <th className="doc-num">{money(m.totalDebit)}</th>
            <th className="doc-num">{money(m.totalCredit)}</th>
            <th />
          </tr>
        </tfoot>
      </table>
      <section className="doc-totals">
        <div className="doc-due">
          <span>{l.closing}</span>
          {money(m.closing)}
        </div>
      </section>
      <footer className="doc-foot">
        <span />
        <span>
          {l.printedAt} <Ltr>{m.printedAt}</Ltr>
        </span>
      </footer>
    </article>
  );
}
