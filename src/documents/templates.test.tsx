import { readFileSync } from "node:fs";
import path from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ArchiveSummary, HostPage } from "@/lib/ipc/types";
import credit from "./__fixtures__/credit.fr.json";
import delivery from "./__fixtures__/delivery.fr.json";
import invoiceAr from "./__fixtures__/invoice.ar.json";
import bilingual from "./__fixtures__/invoice.bilingual.json";
import cancelled from "./__fixtures__/invoice.cancelled.json";
import duplicate from "./__fixtures__/invoice.duplicate.json";
import invoiceEn from "./__fixtures__/invoice.en.json";
import exempt from "./__fixtures__/invoice.exempt.json";
import invoiceFr from "./__fixtures__/invoice.fr.json";
import ifu from "./__fixtures__/invoice.ifu.json";
import proforma from "./__fixtures__/proforma.fr.json";
import purchase from "./__fixtures__/purchase-order.fr.json";
import quote from "./__fixtures__/quote.fr.json";
import receipt from "./__fixtures__/receipt.fr.json";
import statementAr from "./__fixtures__/statement.ar.json";
import statementFr from "./__fixtures__/statement.fr.json";
import tableAr from "./__fixtures__/table.ar.json";
import tableFr from "./__fixtures__/table.fr.json";
import transfer from "./__fixtures__/transfer.fr.json";
import warranty from "./__fixtures__/warranty.fr.json";
import { DocumentPage } from "./page";
import { qrPath } from "./parts";

/** The print stylesheet as written (Vitest does not load CSS). */
const documentsCss = readFileSync(path.resolve(import.meta.dirname, "documents.css"), "utf8");

const doc = (model: unknown): HostPage => ({ template: "document", model }) as HostPage;

const cases: Array<[string, HostPage]> = [
  ["invoice.fr", doc(invoiceFr)],
  ["invoice.ar", doc(invoiceAr)],
  ["invoice.en", doc(invoiceEn)],
  ["invoice.bilingual", doc(bilingual)],
  ["invoice.ifu", doc(ifu)],
  ["invoice.exempt", doc(exempt)],
  ["invoice.duplicate", doc(duplicate)],
  ["invoice.cancelled", doc(cancelled)],
  ["quote.fr", doc(quote)],
  ["proforma.fr", doc(proforma)],
  ["delivery.fr", doc(delivery)],
  ["credit.fr", doc(credit)],
  ["purchase-order.fr", doc(purchase)],
  ["transfer.fr", doc(transfer)],
  ["statement.fr", { template: "statement", model: statementFr } as HostPage],
  ["statement.ar", { template: "statement", model: statementAr } as HostPage],
  ["receipt.fr", { template: "receipt", model: receipt } as HostPage],
  ["table.fr", { template: "table", model: tableFr } as HostPage],
  ["table.ar", { template: "table", model: tableAr } as HostPage],
  ["warranty.fr", { template: "warranty", model: warranty } as HostPage],
];

const html = (page: HostPage) => renderToStaticMarkup(<DocumentPage page={page} />).replace(/></g, ">\n<");

describe("document templates", () => {
  it.each(cases)("%s matches its golden HTML", async (name, page) => {
    await expect(html(page)).toMatchFileSnapshot(`./__golden__/${name}.html`);
  });

  it("prints every mandatory mention of an invoice", () => {
    const out = html(doc(invoiceFr));
    for (const text of [
      "Supérette El Amel",
      "SARL",
      "Commerce de détail en alimentation générale",
      "1\u202F000\u202F000,00\u00A0DA",
      "16/00-1234567B26",
      "000116001234567",
      "SARL Amine",
      "000016001234567",
      "16/00-7654321B26",
      "FA2026-000045",
      "28/09/2026",
      "P.U. HT",
      "TVA 19 %",
      "TVA 9 %",
      "Droit de timbre",
      "Net à payer",
      "23\u202F449,00\u00A0DA",
      "Arrêtée la présente facture à la somme de : vingt-trois mille",
      "À terme (règlement le 28/10/2026)",
      "Cachet et signature",
    ]) {
      expect(out).toContain(text);
    }
  });

  it("never shows TVA on an IFU invoice", () => {
    const out = html(doc(ifu));
    expect(out).toContain("Régime IFU – TVA non applicable");
    expect(out).not.toContain("TVA 19");
    expect(out).not.toContain("P.U. HT");
  });

  it("lays Arabic out right to left and keeps numbers left to right", () => {
    for (const page of [doc(invoiceAr), { template: "statement", model: statementAr } as HostPage]) {
      const out = html(page);
      expect(out).toMatch(/^<article[^>]* dir="rtl"/);
      expect(out).toMatch(/<bdi dir="ltr">FA2026-000045<\/bdi>|<bdi dir="ltr">[^<]*<\/bdi>/);
    }
  });

  it("pairs both languages on a bilingual invoice", () => {
    const out = html(doc(bilingual));
    expect(out).toContain("رقم التعريف الجبائي");
    expect(out).toContain("إسمنت 25 كغ");
    expect(out).toContain('lang="ar"');
  });

  it("repeats the cancelled mark and the DUPLICATA on every printed sheet", () => {
    // A fixed box is printed on each page by Chromium; on screen the mark stays in place.
    const print = /@media print\s*\{([\s\S]*?)\n\}/.exec(documentsCss)?.[1] ?? "";
    expect(print).toMatch(/\.doc-watermark\s*\{[^}]*position:\s*fixed/);
    expect(print).toMatch(/\.doc-copy-mark\s*\{[^}]*position:\s*fixed/);
    expect(html(doc(duplicate))).toContain('class="doc-copy-mark"');
    expect(html(doc(invoiceFr))).not.toContain("doc-copy-mark");
  });

  it("marks a copy, a cancelled invoice and a proforma", () => {
    expect(html(doc(duplicate))).toContain("DUPLICATA");
    expect(html(doc(cancelled))).toContain("FACTURE ANNULÉE — avoir AV2026-000003");
    expect(html(doc(proforma))).toContain("FACTURE PROFORMA — ne constitue pas une facture");
  });

  it("uses no physical direction class", () => {
    for (const [, page] of cases) {
      expect(html(page)).not.toMatch(/class="[^"]*\b(ml|mr|pl|pr|left|right)-|text-(left|right)\b/);
    }
  });

  it("states an exempt buyer's prices HT, without the TVA column (décret 05-468 art. 3)", () => {
    const out = html(doc(exempt));
    for (const word of ["P.U. HT", "Montant HT", "Total HT"]) expect(out).toContain(word);
    expect(out).not.toContain("P.U. TTC");
    expect(html(doc(ifu))).not.toContain("Total HT");
  });

  it("prints the VAT exemption only on a document without TVA", () => {
    const mention = "Exonéré de TVA — attestation n° ATT-2026-114";
    expect(html(doc(exempt))).toContain(mention);
    expect(html(doc({ ...invoiceFr, exemptMention: mention }))).not.toContain(mention);
  });

  it("numbers the pages with the document's word", () => {
    expect(html(doc(invoiceAr))).toMatch(/content:"صفحة " counter\(page\) " \/ " counter\(pages\)/);
    expect(html({ template: "table", model: tableFr } as HostPage)).toContain(
      'content:"Page " counter(page)',
    );
  });

  it("names a receipt's tenders and its change", () => {
    const out = html({ template: "receipt", model: receipt } as HostPage);
    expect(out).toContain("Espèces");
    expect(out).toContain("Monnaie rendue");
    expect(out).toContain("TK2026-C1-000123");
  });

  it("prints a tag through its own list and a day as dd/mm/yyyy", () => {
    const model = {
      ...tableFr,
      columns: [...tableFr.columns, { key: "method", label: "Mode", kind: "tag", tag: "payment_method" }],
      rows: [["2026-09-01", 1_200_000, "transfer"]],
      totals: null,
      tagLabels: { "doc_type.transfer": "Bon de transfert", "payment_method.transfer": "Virement" },
    };
    const out = html({ template: "table", model } as HostPage);
    expect(out).toContain("Virement");
    expect(out).not.toContain("Bon de transfert");
    expect(out).toContain("01/09/2026");
  });

  it("draws the QR as one path, a rectangle per run of dark modules", () => {
    expect(qrPath({ size: 3, modules: "110001111" })).toBe("M0 0h2v1h-2zM2 1h1v1h-1zM0 2h3v1h-3z");
  });

  it("prints the fiscal archive summary with its chains and last hash", () => {
    const model: ArchiveSummary = {
      lang: "fr",
      rtl: false,
      title: "Archive fiscale",
      company: { name: "Supérette El Amel", nif: "000116001234567", nis: null, rc: "16/00-1234567B26" },
      period: "01/01/2026 – 31/12/2026",
      createdAt: "30/09/2026 10:00",
      parts: [{ label: "Factures", rows: 45 }],
      chains: [
        { label: "Factures", series: "FA2026", rows: 45, intact: true },
        { label: "Avoirs", series: "AV2026", rows: 3, intact: false },
      ],
      auditRows: 812,
      auditIntact: true,
      auditLastHash: "9f2c7a",
      labels: {
        period: "Période",
        createdAt: "Créée le",
        parts: "Fichiers",
        rows: "Lignes",
        chains: "Chaînes",
        series: "Série",
        intact: "Intacte",
        broken: "Altérée",
        audit: "Journal d'audit",
        lastHash: "Dernière empreinte",
        nif: "NIF",
        nis: "NIS",
        rc: "RC",
      },
    };
    const out = html({ template: "archive_summary", model });
    expect(out).toMatch(/^<article[^>]* dir="ltr"/);
    expect(out).toContain("Intacte");
    expect(out).toContain("Altérée");
    expect(out).toContain('<bdi dir="ltr">9f2c7a</bdi>');
    // the company's identifiers are labelled; the missing NIS is left out
    expect(out.split("<span>NIF</span>")[1]?.trimStart()).toMatch(/^<bdi dir="ltr">000116001234567</);
    expect(out).not.toContain("NIS");
  });
});
