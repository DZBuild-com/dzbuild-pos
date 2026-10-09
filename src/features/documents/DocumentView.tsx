import { ArrowLeft, Eye, FileDown, Printer, ReceiptText, ShieldCheck, Undo2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useLocation, useParams } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { Button } from "@/components/ui/button";
import { QuoteToOrder } from "@/features/customer-orders/QuoteToOrder";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useHotkeys } from "@/lib/hotkeys";
import { toAppError } from "@/lib/ipc/client";
import { useArchive, useDocLinks, useDocState, useWarrantyLines } from "@/lib/ipc/billing";
import { useDocument } from "@/lib/ipc/documents";
import { useModule } from "@/lib/ipc/pack-state";
import { useExportDocumentPdf } from "@/lib/ipc/print";
import type { DocType, Document, LinkedDocument, Permission, Session } from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { ConvertMenu } from "./ConvertMenu";
import { CreditNoteDialog } from "./CreditNoteDialog";
import { DocumentEditor, EDITOR_KINDS } from "./editor/DocumentEditor";
import { InvoiceDeliveriesDialog } from "./InvoiceDeliveriesDialog";
import { InvoiceTicketDialog } from "./InvoiceTicketDialog";
import { mayOutput, newKinds } from "./kinds";
import { PreviewDialog } from "./PreviewDialog";
import { PrintDialog } from "./PrintDialog";
import { StateBadge } from "./StateBadge";
import { WarrantyDialog } from "./WarrantyDialog";

/** `/documents/:id`: the document of the route, anew for each visit (each id, each navigation). */
export function DocumentRoute({ session }: { session: Session }) {
  const { id = "" } = useParams();
  const { key } = useLocation();
  return <DocumentOrDraft key={`${id}:${key}`} id={id} session={session} />;
}

/** A draft of an invoice, a quote, a proforma or a delivery note opens in its editor, any other
 * document in its view. Decided once: a draft validated in its editor stays there (its print
 * dialog open) until the editor moves on. */
function DocumentOrDraft({ id, session }: { id: string; session: Session }) {
  const { t } = useTranslation();
  const doc = useDocument(id);
  const [draft, setDraft] = useState<DocType | null | undefined>(undefined);
  if (draft === undefined && doc.data) {
    const d = doc.data;
    setDraft(d.status === "draft" && EDITOR_KINDS.includes(d.docType) ? d.docType : null);
  }
  if (draft) return <DocumentEditor kind={draft} draftId={id} session={session} />;
  if (draft === null || doc.error) return <DocumentView id={id} session={session} />;
  return <p className="text-ink-2">{t("common:status.loading")}</p>;
}

/**
 * One document of the archive (DOC-017/018): its header, state, lines and totals as the backend
 * stored them, the documents it was made from and those made from it, and — for the kinds the
 * user may output — Print (F9), PDF and Preview. A validated document printed or saved before
 * says its next copy is a DUPLICATA. A draft only previews (marked BROUILLON). A validated invoice
 * not yet fully credited takes a credit note (its only correction, DOC-006/007); a validated ticket
 * or invoice with a warranted line prints its warranty certificate (DOC-012).
 */
export function DocumentView({ id, session }: { id: string; session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const doc = useDocument(id);
  const state = useDocState(id);
  const links = useDocLinks(id);
  const pdf = useExportDocumentPdf();
  const [dialog, setDialog] = useState<
    "print" | "preview" | "deliveries" | "ticket" | "credit" | "warranty" | null
  >(null);
  const d = doc.data;
  // The archive row of the document: its customer or supplier and the copies made so far.
  const archive = useArchive({ kinds: d ? [d.docType] : [], text: d?.number ?? null }, Boolean(d?.number));
  const row = archive.data?.pages.flatMap((page) => page.rows).find((r) => r.id === id);
  const validated = d?.status === "validated";
  const canOutput = d ? mayOutput(d.docType, session.permissions) : false;
  const has = (p: Permission) => session.permissions.includes(p);
  // The warranted lines of a validated ticket or invoice, for whoever may reprint the one or
  // handle the other (the backend's rule); an error (none warranted) shows no button.
  const warrantable =
    validated &&
    (d?.docType === "ticket" || d?.docType === "invoice") &&
    (has("sales.reprint") || has("documents.invoice"));
  const warranty = useWarrantyLines(warrantable ? id : null);
  useHotkeys({ F9: () => setDialog("print") }, validated && canOutput);
  // M8 (ORD-001): a validated quote or proforma also becomes a customer order
  const ordersOn = useModule("orders");

  if (doc.error) {
    return (
      <p role="alert" className="text-danger">
        {toAppError(doc.error).code === "document_not_found"
          ? t("billing:view.notFound")
          : errorText(doc.error)}
      </p>
    );
  }
  if (!d) return <p className="text-ink-2">{t("common:status.loading")}</p>;

  const savePdf = () =>
    pdf.mutate(
      { id },
      {
        // `null`: the save dialog was cancelled.
        onSuccess: (saved) => {
          if (saved) toast.success(t("billing:print.pdfSaved", { path: saved.path }));
        },
        onError: (error) => toast.error(errorText(error)),
      },
    );
  const party = row?.partyName ?? d.parties.find((p) => p.role === "buyer")?.name ?? null;
  const copies = row?.outputs ?? 0;
  const invoices = has("documents.invoice");
  const ticketInvoiced = state.data?.state === "invoiced";
  const converts = (d.docType === "quote" || d.docType === "proforma") && state.data !== undefined;
  const invoicesDeliveries = d.docType === "delivery_note" && d.customerId !== null && invoices;
  const invoicesTicket = d.docType === "ticket" && invoices;
  const credits =
    d.docType === "invoice" &&
    has("documents.credit_note") &&
    state.data !== undefined &&
    state.data.state !== "credited";
  const warranted = (warranty.data?.length ?? 0) > 0;

  return (
    <div className="flex flex-col gap-4">
      <Link
        to="/documents"
        className="inline-flex min-h-11 items-center gap-2 self-start rounded-control font-semibold text-ink-2 hover:text-ink focus-visible:outline-3 focus-visible:outline-focus"
      >
        <ArrowLeft aria-hidden className="size-5 rtl:rotate-180" />
        {t("billing:list.title")}
      </Link>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex flex-col gap-2">
          <h1 className="text-3xl font-bold">
            {t(`billing:kinds.${d.docType}`)} {d.number ? <bdi dir="ltr">{d.number}</bdi> : null}
          </h1>
          {state.data ? (
            <div>
              <StateBadge state={state.data} />
            </div>
          ) : null}
        </div>
        {canOutput ? (
          <div className="flex flex-wrap gap-2">
            {validated ? (
              <>
                <Button variant="primary" shortcut="F9" onClick={() => setDialog("print")}>
                  <Printer aria-hidden />
                  {t("billing:view.print")}
                </Button>
                <Button variant="secondary" disabled={pdf.isPending} onClick={savePdf}>
                  <FileDown aria-hidden />
                  {t("billing:view.pdf")}
                </Button>
              </>
            ) : null}
            <Button variant="outline" onClick={() => setDialog("preview")}>
              <Eye aria-hidden />
              {t("billing:view.preview")}
            </Button>
          </div>
        ) : null}
      </div>
      <dl className="grid max-w-xl grid-cols-[auto_1fr] gap-x-6 gap-y-1">
        <dt className="text-ink-2">{t("billing:list.columns.date")}</dt>
        <dd>
          <bdi dir="ltr">{formatDate(dayToMs(d.businessDate), lang)}</bdi>
        </dd>
        {party ? (
          <>
            <dt className="text-ink-2">{t("billing:list.columns.party")}</dt>
            <dd>{party}</dd>
          </>
        ) : null}
      </dl>
      {validated && (converts || invoicesDeliveries || invoicesTicket || credits || warranted) ? (
        <div className="flex flex-wrap items-start gap-3">
          {state.data ? (
            <ConvertMenu doc={d} state={state.data} allowed={newKinds(session.permissions)} />
          ) : null}
          {state.data && converts && ordersOn && has("documents.quote") ? (
            <QuoteToOrder doc={d} state={state.data} />
          ) : null}
          {invoicesDeliveries ? (
            <Button variant="secondary" onClick={() => setDialog("deliveries")}>
              <ReceiptText aria-hidden />
              {t("billing:convert.invoiceDeliveries")}
            </Button>
          ) : null}
          {invoicesTicket ? (
            <div className="flex flex-col gap-1">
              <Button variant="secondary" disabled={ticketInvoiced} onClick={() => setDialog("ticket")}>
                <ReceiptText aria-hidden />
                {t("billing:ticketInvoice.action")}
              </Button>
              {ticketInvoiced ? (
                <p className="text-sm text-ink-2">{t("billing:ticketInvoice.already")}</p>
              ) : null}
            </div>
          ) : null}
          {credits ? (
            <Button variant="secondary" onClick={() => setDialog("credit")}>
              <Undo2 aria-hidden className="rtl:-scale-x-100" />
              {t("billing:credit.action")}
            </Button>
          ) : null}
          {warranted ? (
            <Button variant="secondary" onClick={() => setDialog("warranty")}>
              <ShieldCheck aria-hidden />
              {t("billing:warranty.action")}
            </Button>
          ) : null}
        </div>
      ) : null}
      {validated && copies > 0 ? (
        <p role="note" className="rounded-control border border-warn bg-warn-soft p-3 text-warn">
          {t("billing:view.copies", { count: copies })}
        </p>
      ) : null}
      <Lines doc={d} />
      <Totals doc={d} />
      <Linked title={t("billing:view.sources")} docs={links.data?.sources ?? []} end="from" />
      <Linked title={t("billing:view.targets")} docs={links.data?.targets ?? []} end="to" />
      {dialog === "print" ? (
        <PrintDialog id={id} docType={d.docType} open onClose={() => setDialog(null)} />
      ) : null}
      {dialog === "credit" ? <CreditNoteDialog invoice={d} open onClose={() => setDialog(null)} /> : null}
      {dialog === "warranty" ? <WarrantyDialog documentId={id} open onClose={() => setDialog(null)} /> : null}
      {dialog === "deliveries" && d.customerId !== null ? (
        <InvoiceDeliveriesDialog customerId={d.customerId} onClose={() => setDialog(null)} />
      ) : null}
      {dialog === "ticket" ? (
        <InvoiceTicketDialog
          ticketId={id}
          ticketNumber={d.number ?? ""}
          open
          onClose={() => setDialog(null)}
        />
      ) : null}
      {dialog === "preview" ? (
        <PreviewDialog
          id={id}
          options={{}}
          open
          onClose={() => setDialog(null)}
          onPrint={validated ? () => setDialog("print") : undefined}
        />
      ) : null}
    </div>
  );
}

function Lines({ doc }: { doc: Document }) {
  const { t } = useTranslation();
  const lang = useLang();
  return (
    <section aria-labelledby="document-lines" className="flex flex-col gap-2">
      <h2 id="document-lines" className="text-xl font-bold">
        {t("billing:view.lines")}
      </h2>
      <div className="overflow-x-auto rounded-card border border-line bg-surface">
        <table className="w-full border-collapse text-sm">
          <thead className="bg-surface-2 text-ink-2">
            <tr className="h-10">
              <th className="px-3 text-start font-semibold">{t("billing:view.columns.description")}</th>
              <th className="px-3 text-end font-semibold">{t("billing:view.columns.qty")}</th>
              <th className="px-3 text-end font-semibold">{t("billing:view.columns.unitPrice")}</th>
              <th className="px-3 text-end font-semibold">{t("billing:list.columns.totalTtc")}</th>
            </tr>
          </thead>
          <tbody>
            {doc.lines
              .filter((l) => l.parentLineId === null)
              .map((l) => (
                <tr key={l.id} className="h-10 border-t border-line even:bg-surface-2">
                  <td className="px-3">
                    {l.description}
                    {l.descriptionAlt ? (
                      <span dir="auto" className="block text-ink-2">
                        {l.descriptionAlt}
                      </span>
                    ) : null}
                  </td>
                  <td className="px-3 text-end tabular-nums">
                    <bdi dir="ltr">{formatQty(l.qty, lang)}</bdi>
                  </td>
                  <td className="px-3 text-end">
                    <Amount value={l.unitPrice} />
                  </td>
                  <td className="px-3 text-end">
                    <Amount value={l.totalTtc} />
                  </td>
                </tr>
              ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

/** The backend's totals; an IFU document never shows TVA (CTCA art. 64). */
function Totals({ doc }: { doc: Document }) {
  const { t } = useTranslation();
  const rows: { label: string; value: number; strong?: boolean }[] = [];
  if (doc.taxRegime === "reel") {
    rows.push({ label: t("billing:view.totalHt"), value: doc.subtotalHt });
    rows.push({ label: t("billing:view.tax"), value: doc.taxTotal });
  }
  rows.push({ label: t("billing:list.columns.totalTtc"), value: doc.totalTtc });
  if (doc.stampDuty !== 0) rows.push({ label: t("billing:view.stampDuty"), value: doc.stampDuty });
  rows.push({ label: t("billing:list.columns.totalDue"), value: doc.totalDue, strong: true });
  return (
    <section aria-labelledby="document-totals" className="flex flex-col gap-2 self-end">
      <h2 id="document-totals" className="sr-only">
        {t("billing:view.totals")}
      </h2>
      <dl className="grid min-w-72 grid-cols-[1fr_auto] gap-x-6 gap-y-1">
        {rows.map(({ label, value, strong }) => (
          <div key={label} className={strong ? "contents text-lg font-bold" : "contents"}>
            <dt>{label}</dt>
            <dd className="text-end">
              <Amount value={value} />
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

/** The documents at the other end of a document's links, each opening its own view. */
function Linked({ title, docs, end }: { title: string; docs: LinkedDocument[]; end: "from" | "to" }) {
  const { t } = useTranslation();
  const lang = useLang();
  if (docs.length === 0) return null;
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-xl font-bold">{title}</h2>
      <ul className="flex flex-col gap-1">
        {docs.map((linked) => (
          <li key={linked.link.id}>
            <Link
              to={`/documents/${end === "from" ? linked.link.fromId : linked.link.toId}`}
              className="inline-flex min-h-11 flex-wrap items-center gap-3 rounded-control px-2 hover:bg-surface-2 focus-visible:outline-3 focus-visible:outline-focus"
            >
              <span className="font-semibold">{t(`billing:kinds.${linked.docType}`)}</span>
              <bdi dir="ltr">{linked.number ?? ""}</bdi>
              <bdi dir="ltr" className="text-ink-2">
                {formatDate(dayToMs(linked.businessDate), lang)}
              </bdi>
              <Amount value={linked.totalDue} />
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}
