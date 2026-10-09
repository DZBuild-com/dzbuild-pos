import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useArchive, useDeliveriesPreview, useInvoiceDeliveries, useMentions } from "@/lib/ipc/billing";
import type { Document, PaymentInput } from "@/lib/ipc/types";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { MentionsPanel, isComplete } from "./editor/MentionsPanel";
import { SettlementFigures, TendersEditor } from "./editor/TendersEditor";
import { PrintDialog } from "./PrintDialog";

/** The most delivery notes one invoice gathers (`billing::convert::MAX_DELIVERIES`). */
const MAX_DELIVERIES = 50;

/**
 * DOC-031: the customer's delivery notes still to invoice, ticked (50 at most) into one invoice
 * with its due date and payments; the notes already moved the stock, the invoice does not. The
 * backend answers every figure as the tenders are typed (its total, the duty on the cash part,
 * the net to pay, what is left), and what the customer's details still lack blocks it like any
 * invoice (DOC-003). Once made, its print dialog opens, then its view.
 */
export function InvoiceDeliveriesDialog({
  customerId,
  onClose,
}: {
  customerId: string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const navigate = useNavigate();
  const archive = useArchive({ kinds: ["delivery_note"], customerId, status: "validated" });
  const rows = useMemo(
    () =>
      (archive.data?.pages.flatMap((page) => page.rows) ?? []).filter((r) => r.state.state === "to_invoice"),
    [archive.data],
  );
  const [ticked, setTicked] = useState<string[]>([]);
  const [dueDate, setDueDate] = useState("");
  const [tenders, setTenders] = useState<PaymentInput[]>([]);
  const [invoice, setInvoice] = useState<Document | null>(null);
  const make = useInvoiceDeliveries();
  // oldest first, as the lines follow on the invoice
  const deliveryIds = useMemo(
    () =>
      rows
        .filter((r) => ticked.includes(r.id))
        .map((r) => r.id)
        .reverse(),
    [rows, ticked],
  );
  const typed = useDebounced(tenders, 200);
  const settlement = useDeliveriesPreview(deliveryIds, typed);
  const mentions = useMentions(customerId);
  const check = mentions.data;
  const short = settlement.data !== undefined && settlement.data.missing > 0;
  const blocked = check !== undefined && !isComplete(check);
  if (invoice) {
    return (
      <PrintDialog
        id={invoice.id}
        docType={invoice.docType}
        doc={invoice}
        open
        onClose={() => void navigate(`/documents/${invoice.id}`)}
      />
    );
  }
  const full = ticked.length >= MAX_DELIVERIES;
  const toggle = (id: string, on: boolean) =>
    setTicked((all) => (on ? (all.includes(id) ? all : [...all, id]) : all.filter((x) => x !== id)));
  const submit = () =>
    make.mutate(
      {
        deliveryIds,
        payments: tenders,
        dueDate: dueDate === "" ? null : dueDate,
        secondLang: null,
        note: null,
      },
      {
        onSuccess: (done) => {
          toast.success(t("billing:validate.done", { number: done.document.number ?? "" }));
          setInvoice(done.document);
        },
      },
    );
  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("billing:deliveries.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={ticked.length === 0 || make.isPending || short || blocked || settlement.isFetching}
            onClick={submit}
          >
            {t("billing:deliveries.submit")}
          </Button>
        </>
      }
    >
      <fieldset className="flex flex-col gap-2">
        <legend className="mb-1 font-bold">{t("billing:deliveries.pick")}</legend>
        <p className="text-sm text-ink-2">{t("billing:deliveries.limit")}</p>
        {archive.isSuccess && rows.length === 0 ? (
          <p className="text-ink-2">{t("billing:deliveries.none")}</p>
        ) : null}
        <ul className="flex flex-col gap-1">
          {rows.map((row) => {
            const on = ticked.includes(row.id);
            const id = `delivery-${row.id}`;
            return (
              <li key={row.id} className="flex min-h-11 items-center gap-3">
                <Checkbox
                  id={id}
                  checked={on}
                  disabled={!on && full}
                  onCheckedChange={(value) => toggle(row.id, value === true)}
                />
                <label htmlFor={id} className="flex flex-1 flex-wrap items-center gap-3">
                  <bdi dir="ltr" className="font-semibold">
                    {row.number ?? ""}
                  </bdi>
                  <bdi dir="ltr" className="text-ink-2">
                    {formatDate(dayToMs(row.businessDate), lang)}
                  </bdi>
                  <Amount value={row.totalDue} className="ms-auto" />
                </label>
              </li>
            );
          })}
        </ul>
        {archive.hasNextPage ? (
          <div>
            <Button
              variant="outline"
              disabled={archive.isFetchingNextPage}
              onClick={() => void archive.fetchNextPage()}
            >
              {t("common:actions.loadMore")}
            </Button>
          </div>
        ) : null}
      </fieldset>
      <Field id="deliveries-due" label={t("billing:editor.dueDate")}>
        <Input
          id="deliveries-due"
          type="date"
          dir="ltr"
          value={dueDate}
          onChange={(event) => setDueDate(event.target.value)}
        />
      </Field>
      {check ? <MentionsPanel check={check} docType="invoice" customerId={customerId} /> : null}
      <TendersEditor withCredit onChange={setTenders} disabled={make.isPending} />
      {deliveryIds.length > 0 && settlement.data ? <SettlementFigures settlement={settlement.data} /> : null}
      {settlement.error ? (
        <p role="alert" className="text-danger">
          {errorText(settlement.error)}
        </p>
      ) : null}
      {make.error ? (
        <p role="alert" className="text-danger">
          {errorText(make.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
