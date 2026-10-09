import type { ArchiveSummary } from "@/lib/ipc/types";
import { formatNumber } from "@/lib/format";
import { Ltr } from "./parts";

/** `summary.pdf` of the fiscal archive: the company, the period, every file with its rows, every
 * document chain intact or altered, and the audit journal's state and last hash. */
export function ArchiveSummaryDocument({ m }: { m: ArchiveSummary }) {
  const lang = m.lang;
  const l = m.labels;
  const count = (n: number) => <Ltr>{formatNumber(n, lang)}</Ltr>;
  const state = (intact: boolean) => (
    <span className={intact ? "doc-ok" : "doc-broken"}>{intact ? l.intact : l.broken}</span>
  );
  const ids: Array<[string, string | null]> = [
    [l.nif, m.company.nif],
    [l.nis, m.company.nis],
    [l.rc, m.company.rc],
  ];
  return (
    <article className="doc" lang={lang} dir={m.rtl ? "rtl" : "ltr"} data-accent="ink">
      <header className="doc-head">
        <div>
          <h1 className="doc-title">{m.title}</h1>
          <p>
            <strong>{m.company.name}</strong>
          </p>
          <div className="doc-ids">
            {ids.map(([label, value]) =>
              value ? (
                <span key={label} className="doc-id">
                  <span>{label}</span>
                  <Ltr>{value}</Ltr>
                </span>
              ) : null,
            )}
          </div>
        </div>
        <div>
          <p>
            {l.period} : <Ltr>{m.period}</Ltr>
          </p>
          <p>
            {l.createdAt} : <Ltr>{m.createdAt}</Ltr>
          </p>
        </div>
      </header>
      <h2 className="doc-legal">{l.parts}</h2>
      <table className="doc-lines">
        <thead>
          <tr>
            <th>{l.parts}</th>
            <th className="doc-num">{l.rows}</th>
          </tr>
        </thead>
        <tbody>
          {m.parts.map((part) => (
            <tr key={part.label}>
              <td>
                <Ltr>{part.label}</Ltr>
              </td>
              <td className="doc-num">{count(part.rows)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <h2 className="doc-legal">{l.chains}</h2>
      <table className="doc-lines">
        <thead>
          <tr>
            <th>{l.chains}</th>
            <th>{l.series}</th>
            <th className="doc-num">{l.rows}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {m.chains.map((chain) => (
            <tr key={`${chain.label}-${chain.series}`}>
              <td>{chain.label}</td>
              <td>
                <Ltr>{chain.series}</Ltr>
              </td>
              <td className="doc-num">{count(chain.rows)}</td>
              <td>{state(chain.intact)}</td>
            </tr>
          ))}
          <tr>
            <td colSpan={2}>{l.audit}</td>
            <td className="doc-num">{count(m.auditRows)}</td>
            <td>{state(m.auditIntact)}</td>
          </tr>
        </tbody>
      </table>
      <p>
        {l.lastHash} : <Ltr>{m.auditLastHash}</Ltr>
      </p>
    </article>
  );
}
