import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Amount } from "@/components/amount";
import { LanguageToggle } from "@/components/language-toggle";
import { Button } from "@/components/ui/button";
import { Sheet } from "@/components/ui/sheet";
import { formatDate } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import { useDocState } from "@/lib/ipc/billing";
import { useDocument, useReprint } from "@/lib/ipc/documents";
import type { Session } from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { InvoiceTicketDialog } from "./InvoiceTicketDialog";

/** One ticket or return note: lines, taxes, payments, its chain hash; DUPLICATA reprint in any
 * language (DOC-018, UX-004); items back through the return flow (RET-002); a ticket's invoice
 * for its customer (SAL-054, `documents.invoice`). */
export function DocumentSheet({
  id,
  session,
  onClose,
}: {
  id: string;
  session: Session;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const errorText = useErrorText();
  const doc = useDocument(id);
  const reprint = useReprint();
  const [printLang, setPrintLang] = useState<Lang | null>(null);
  const d = doc.data;
  const canReprint = session.permissions.includes("sales.reprint");
  const canReturn = session.permissions.includes("sales.create") && d?.docType === "ticket";
  const canInvoice = session.permissions.includes("documents.invoice") && d?.docType === "ticket";
  // Whether the ticket has its invoice (readable with `sales.view_all`; else the backend refuses
  // a second invoice itself).
  const state = useDocState(canInvoice ? id : null);
  const invoiced = state.data?.state === "invoiced";
  const [invoicing, setInvoicing] = useState(false);
  return (
    <Sheet
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("documents:sheet.title", { number: d?.number ?? "" })}
    >
      {d ? (
        <div className="flex flex-col gap-4">
          <p className="text-ink-2">
            <bdi dir="ltr">{d.validatedAt ? formatDate(d.validatedAt, lang, { withTime: true }) : ""}</bdi>
          </p>
          <ul className="flex flex-col gap-2">
            {d.lines
              .filter((l) => l.parentLineId === null)
              .map((l) => (
                <li key={l.id} className="flex justify-between gap-3">
                  <span>
                    <bdi dir="ltr">{formatQty(Math.abs(l.qty), lang)}</bdi> × <span>{l.description}</span>
                  </span>
                  <Amount value={l.totalTtc} />
                </li>
              ))}
          </ul>
          <dl className="grid gap-1 border-t border-line pt-3">
            {d.taxRegime === "reel" ? (
              <div className="flex justify-between">
                <dt>{t(d.priceMode === "ht" ? "pos:cart.vatAdded" : "pos:cart.vat")}</dt>
                <dd>
                  <Amount value={d.taxTotal} />
                </dd>
              </div>
            ) : null}
            {d.stampDuty !== 0 ? (
              <div className="flex justify-between">
                <dt>{t("payments:stampDuty")}</dt>
                <dd>
                  <Amount value={d.stampDuty} />
                </dd>
              </div>
            ) : null}
            <div className="flex justify-between font-bold">
              <dt>{t("documents:sheet.total")}</dt>
              <dd>
                <Amount value={d.totalDue} />
              </dd>
            </div>
            {d.payments.map((p) => (
              <div key={p.id} className="flex justify-between">
                <dt>{t(`payments:tenders.${p.method}`)}</dt>
                <dd>
                  <Amount value={p.amount} />
                </dd>
              </div>
            ))}
          </dl>
          <p className="text-sm text-ink-2">
            {t("documents:sheet.hash")}{" "}
            <bdi dir="ltr" className="font-mono">
              {d.hash?.slice(0, 16)}
            </bdi>
          </p>
          {canReprint ? (
            <div className="flex flex-col gap-2">
              <span className="font-semibold">{t("documents:sheet.printLang")}</span>
              <LanguageToggle value={printLang ?? d.lang} onChange={setPrintLang} />
              <Button
                variant="primary"
                disabled={reprint.isPending}
                onClick={() => reprint.mutate({ id: d.id, lang: printLang })}
              >
                {t("documents:sheet.reprint")}
              </Button>
              {reprint.data ? (
                <p role="status" className="text-ok">
                  {t("documents:sheet.reprinted", { printer: reprint.data.printer })}
                </p>
              ) : null}
              {reprint.error ? (
                <p role="alert" className="text-danger">
                  {errorText(reprint.error)}
                </p>
              ) : null}
            </div>
          ) : null}
          {canInvoice ? (
            <div className="flex flex-col gap-1">
              <Button variant="outline" disabled={invoiced} onClick={() => setInvoicing(true)}>
                {t("billing:ticketInvoice.action")}
              </Button>
              {invoiced ? <p className="text-sm text-ink-2">{t("billing:ticketInvoice.already")}</p> : null}
            </div>
          ) : null}
          {canReturn ? (
            <Button
              variant="outline"
              onClick={() => void navigate("/pos", { state: { returnNumber: d.number } })}
            >
              {t("documents:sheet.return")}
            </Button>
          ) : null}
        </div>
      ) : null}
      {invoicing && d ? (
        <InvoiceTicketDialog
          ticketId={d.id}
          ticketNumber={d.number ?? ""}
          open
          onClose={() => setInvoicing(false)}
        />
      ) : null}
      {doc.error ? (
        <p role="alert" className="text-danger">
          {errorText(doc.error)}
        </p>
      ) : null}
    </Sheet>
  );
}
