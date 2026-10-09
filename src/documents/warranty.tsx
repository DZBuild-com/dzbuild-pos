import type { WarrantyPrint } from "@/lib/ipc/types";
import { docMoney, docQty } from "./format";
import { Ltr, PageNumbers, PartyCard, QrCode, type Words } from "./parts";

/** The warranty certificate of décret 13-327 art. 6: guarantor with its RC, buyer, purchase
 * document, goods (type, brand, serial numbers), price, duration and the representative's box. */
export function WarrantyDocument({ m }: { m: WarrantyPrint }) {
  const lang = m.lang;
  const l = m.labels;
  const words: Words = { lang, labels: m.partyLabels, second: null };
  return (
    <article className="doc" lang={lang} dir={m.rtl ? "rtl" : "ltr"} data-accent="ink">
      <PageNumbers label={m.partyLabels.page} />
      <header className="doc-head">
        <div>
          <h1 className="doc-title">{m.title}</h1>
          <p>
            {l.document} : {m.documentTitle} <Ltr>{m.documentNumber}</Ltr> — <Ltr>{m.documentDate}</Ltr>
          </p>
        </div>
        {m.qr ? <QrCode qr={m.qr} label={m.documentNumber} /> : null}
      </header>
      <div className="doc-parties">
        <PartyCard m={words} p={m.seller} title={l.guarantor} />
        {m.buyer ? (
          <PartyCard m={words} p={m.buyer} title={l.buyer} />
        ) : (
          <section className="doc-party doc-fill">
            <h2 className="doc-legal">{l.buyer}</h2>
            <p>{m.partyLabels.customer} :</p>
            <p>{m.partyLabels.address} :</p>
          </section>
        )}
      </div>
      <table className="doc-lines">
        <tbody>
          <tr>
            <th>{l.product}</th>
            <td>
              <bdi>{m.product}</bdi>
              {m.qty !== 1000 ? (
                <>
                  {" × "}
                  <Ltr>{docQty(m.qty, lang)}</Ltr>
                </>
              ) : null}
            </td>
          </tr>
          {m.brand ? (
            <tr>
              <th>{l.brand}</th>
              <td>
                <bdi>{m.brand}</bdi>
              </td>
            </tr>
          ) : null}
          {m.serials.length ? (
            <tr>
              <th>{l.serial}</th>
              <td>
                {m.serials.map((serial) => (
                  <div key={serial}>
                    <Ltr>{serial}</Ltr>
                  </div>
                ))}
              </td>
            </tr>
          ) : null}
          <tr>
            <th>{l.price}</th>
            <td>
              <Ltr>{docMoney(m.priceTtc, lang)}</Ltr>
            </td>
          </tr>
          <tr>
            <th>{l.duration}</th>
            <td>
              <Ltr>{m.months}</Ltr> {l.months}
            </td>
          </tr>
          <tr>
            <th>{l.endsOn}</th>
            <td>
              <Ltr>{m.endsOn}</Ltr>
            </td>
          </tr>
        </tbody>
      </table>
      {m.conditions ? (
        <section>
          <h2 className="doc-legal">{l.conditions}</h2>
          <p>{m.conditions}</p>
        </section>
      ) : null}
      <div className="doc-stamp">
        <p>{l.representative}</p>
        <p>{l.signature}</p>
      </div>
      <footer className="doc-foot">
        <span />
        <span>
          {m.partyLabels.printedAt} <Ltr>{m.printedAt}</Ltr>
        </span>
      </footer>
    </article>
  );
}
