import type { RouteSheetPrint } from "@/lib/ipc/types";
import { docMoney } from "./format";
import { Ltr, PageNumbers, PartyCard, SignatureBoxes, type Words } from "./parts";

/** The bon de route of a delivery round (VRT-100, DOC-030): the shop, the driver, the vehicle and
 * its stock, then every stop in route order with what to collect, a column for what was collected
 * and the customer's signature, the totals, and the driver's and the shop's signatures. */
export function RouteSheetPage({ model: m }: { model: RouteSheetPrint }) {
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
        <div>
          <p>
            {l.driver} : <bdi>{m.driver}</bdi>
          </p>
          {m.plate ? (
            <p>
              {l.plate} : <Ltr>{m.plate}</Ltr>
            </p>
          ) : null}
          {m.van ? (
            <p>
              {l.van} : <bdi>{m.van}</bdi>
            </p>
          ) : null}
        </div>
      </header>
      <div className="doc-parties">
        <PartyCard m={words} p={m.seller} title={m.partyLabels.seller} />
      </div>
      <table className="doc-lines">
        <thead>
          <tr>
            <th className="doc-num">#</th>
            <th>{l.customer}</th>
            <th>{l.address}</th>
            <th>{l.phone}</th>
            <th>{l.document}</th>
            <th className="doc-num">{l.amountDue}</th>
            <th className="doc-num">{l.collected}</th>
            <th>{l.status}</th>
            <th>{l.signature}</th>
          </tr>
        </thead>
        <tbody>
          {m.stops.map((stop) => (
            <tr key={stop.seq}>
              <td className="doc-num">
                <Ltr>{stop.seq}</Ltr>
              </td>
              <td>
                <bdi>{stop.customer}</bdi>
              </td>
              <td>{stop.address ? <bdi>{stop.address}</bdi> : null}</td>
              <td>{stop.phone ? <Ltr>{stop.phone}</Ltr> : null}</td>
              <td>
                <Ltr>{stop.document}</Ltr>
              </td>
              <td className="doc-num">{money(stop.amountDue)}</td>
              <td className="doc-num">{stop.collected ? money(stop.collected) : null}</td>
              <td>{stop.status}</td>
              <td className="doc-sign-cell" />
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <td colSpan={5} />
            <td className="doc-num">
              <strong>{money(m.totalDue)}</strong>
            </td>
            <td className="doc-num">
              <strong>{money(m.totalCollected)}</strong>
            </td>
            <td colSpan={2} />
          </tr>
        </tfoot>
      </table>
      <SignatureBoxes start={m.signatures[0]} end={m.signatures[1]} />
    </article>
  );
}
