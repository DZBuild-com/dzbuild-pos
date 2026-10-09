import type { ProcessingRegisterPrint } from "@/lib/ipc/types";
import { Ltr, PageNumbers, PartyCard, type Words } from "./parts";

/** The record of processing activities (PDP-005, loi 18-07): the shop, its data protection
 * officer (or the line saying none is named), one row per activity of the modules in use with
 * its legal basis, the data it keeps and how long, and the security measures. Every word comes
 * from the model. */
export function ProcessingRegisterDocument({ m }: { m: ProcessingRegisterPrint }) {
  const lang = m.lang;
  const words: Words = { lang, labels: m.partyLabels, second: null };
  return (
    <article className="doc" lang={lang} dir={m.rtl ? "rtl" : "ltr"} data-accent="ink">
      <PageNumbers label={m.partyLabels.page} />
      <header className="doc-head">
        <div>
          <h1 className="doc-title">{m.title}</h1>
          <p>
            {m.partyLabels.date} <Ltr>{m.date}</Ltr>
          </p>
        </div>
      </header>
      <div className="doc-parties">
        <PartyCard m={words} p={m.seller} />
      </div>
      <p>
        <strong>{m.labels.dpo}</strong> : <bdi>{m.dpo ?? m.labels.noDpo}</bdi>
      </p>
      <table className="doc-lines">
        <thead>
          <tr>
            <th scope="col">{m.labels.activity}</th>
            <th scope="col">{m.labels.basis}</th>
            <th scope="col">{m.labels.data}</th>
            <th scope="col">{m.labels.retention}</th>
          </tr>
        </thead>
        <tbody>
          {m.activities.map(([name, basis, data, retention]) => (
            <tr key={name}>
              <th scope="row">
                <bdi>{name}</bdi>
              </th>
              <td>
                <bdi>{basis}</bdi>
              </td>
              <td>
                <bdi>{data}</bdi>
              </td>
              <td>
                <bdi>{retention}</bdi>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <h2 className="doc-legal">{m.labels.security}</h2>
      <p className="doc-legal">{m.security}</p>
    </article>
  );
}
