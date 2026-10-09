import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { type Lang, LANGS, isLang } from "@/lib/i18n";
import { useInvoiceTicket, useMentions } from "@/lib/ipc/billing";
import { useDocument } from "@/lib/ipc/documents";
import { useSetting } from "@/lib/ipc/settings";
import type { Document } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { CustomerField } from "./editor/CustomerField";
import { defaultSecondLang } from "./editor/editor-store";
import { MentionsPanel, isComplete } from "./editor/MentionsPanel";
import { PrintDialog } from "./PrintDialog";

/**
 * SAL-054: the invoice of a till ticket, for the customer who asks for it (every blocking
 * mention of his details present). The invoice repeats the ticket's lines, totals and payments;
 * the ticket keeps the turnover, the stock and the cash. Its print dialog opens once it exists.
 */
export function InvoiceTicketDialog({
  ticketId,
  ticketNumber,
  open,
  onClose,
}: {
  ticketId: string;
  ticketNumber: string;
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const [customerId, setCustomerId] = useState<string | null>(null);
  // `null` until the user picks: the second language the settings set for new documents.
  const [picked, setSecond] = useState<Lang | "" | null>(null);
  const [note, setNote] = useState("");
  const [invoice, setInvoice] = useState<Document | null>(null);
  // The ticket's language: a bilingual invoice prints another one beside it.
  const ticket = useDocument(open ? ticketId : null);
  const layout = useSetting("documents.layout", open);
  const ticketLang = ticket.data?.lang;
  const second: Lang | "" = picked ?? (ticketLang ? (defaultSecondLang(layout.data, ticketLang) ?? "") : "");
  const mentions = useMentions(customerId, open && customerId !== null);
  const make = useInvoiceTicket();
  if (!open) return null;
  if (invoice) {
    return <PrintDialog id={invoice.id} docType={invoice.docType} doc={invoice} open onClose={onClose} />;
  }
  const check = customerId !== null ? mentions.data : undefined;
  const blocked = customerId === null || (check !== undefined && !isComplete(check));
  const submit = () => {
    if (customerId === null) return;
    make.mutate(
      {
        ticketId,
        customerId,
        secondLang: second === "" ? null : second,
        note: note.trim() === "" ? null : note.trim(),
      },
      {
        onSuccess: (done) => {
          toast.success(t("billing:validate.done", { number: done.document.number ?? "" }));
          setInvoice(done.document);
        },
      },
    );
  };
  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("billing:ticketInvoice.title", { number: ticketNumber })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={blocked || make.isPending} onClick={submit}>
            {t("billing:ticketInvoice.submit")}
          </Button>
        </>
      }
    >
      <CustomerField value={customerId} onChange={setCustomerId} disabled={make.isPending} />
      {check ? <MentionsPanel check={check} docType="invoice" customerId={customerId} /> : null}
      <Field id="ticket-invoice-second" label={t("billing:editor.secondLanguage")}>
        <NativeSelect
          id="ticket-invoice-second"
          value={second}
          onChange={(event) => {
            const value = event.target.value;
            setSecond(isLang(value) ? value : "");
          }}
        >
          <option value="">{t("billing:editor.none")}</option>
          {LANGS.filter((l) => l !== ticket.data?.lang).map((l) => (
            <option key={l} value={l} lang={l}>
              {t(`common:languages.${l}`)}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <Field id="ticket-invoice-note" label={t("billing:editor.note")}>
        <Input
          id="ticket-invoice-note"
          dir="auto"
          maxLength={200}
          value={note}
          onChange={(event) => setNote(event.target.value)}
        />
      </Field>
      {make.error ? (
        <p role="alert" className="text-danger">
          {errorText(make.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
