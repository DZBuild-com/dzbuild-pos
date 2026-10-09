import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useApproval } from "@/components/manager-approval";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { isApprovalCancelled } from "@/lib/approval";
import type { Lang } from "@/lib/i18n";
import { useMentions, usePaymentPreview, useValidateDocument } from "@/lib/ipc/billing";
import type { DocType, PaymentInput, ValidatedDocument } from "@/lib/ipc/types";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { ArabicGuardBanner } from "../ArabicGuardBanner";
import { MentionsPanel } from "./MentionsPanel";
import { SettlementFigures, TendersEditor } from "./TendersEditor";

const NO_TENDERS: PaymentInput[] = [];

/** The saved draft being validated. */
export interface DraftRef {
  id: string;
  docType: DocType;
  customerId: string | null;
  /** Its languages and products, for the Arabic guard (DOC-034). */
  lang: Lang;
  secondLang: Lang | null;
  productIds: string[];
}

/**
 * Validates a saved draft: it gets its number and can no longer change (DOC-001). A quote, a
 * proforma or a delivery note shows what its mentions still lack; an invoice takes its payments
 * (cash, cards, cheque, transfer, "À terme" on the customer's account), the backend answering the
 * duty on the cash part, the net to pay, what is left and the change. A refusal a manager can
 * lift (a credit limit) asks for his PIN and tries again. A document to a private person printed
 * without Arabic shows the Arabic guard's suggestion (DOC-034), which `onSwitchLang` applies to
 * the draft; it never blocks.
 */
export function ValidateDialog({
  draft,
  onClose,
  onValidated,
  onSwitchLang,
}: {
  draft: DraftRef;
  onClose: () => void;
  onValidated: (done: ValidatedDocument) => void;
  onSwitchLang: (lang: Lang) => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const invoice = draft.docType === "invoice";
  const [tenders, setTenders] = useState<PaymentInput[]>(NO_TENDERS);
  const typed = useDebounced(tenders, 200);
  const settlement = usePaymentPreview(invoice ? draft.id : null, invoice ? typed : null);
  const mentions = useMentions(draft.customerId);
  const validate = useValidateDocument();
  const approval = useApproval();
  const [error, setError] = useState<unknown>(null);
  const check = mentions.data;
  const short = invoice && settlement.data !== undefined && settlement.data.missing > 0;

  const submit = () => {
    setError(null);
    approval
      .run(() => validate.mutateAsync({ id: draft.id, payments: invoice ? tenders : [] }))
      .then(
        (done) => {
          toast.success(t("billing:validate.done", { number: done.document.number ?? "" }));
          onValidated(done);
        },
        (failure: unknown) => {
          if (!isApprovalCancelled(failure)) setError(failure);
        },
      );
  };

  return (
    <>
      <Dialog
        open
        size="lg"
        onOpenChange={(open) => (open ? undefined : onClose())}
        title={t(`billing:validate.title.${draft.docType}`)}
        description={t("billing:validate.summary")}
        footer={
          <>
            <Button variant="outline" onClick={onClose}>
              {t("common:actions.cancel")}
            </Button>
            <Button variant="primary" disabled={validate.isPending || short} onClick={submit}>
              {t("billing:validate.submit")}
            </Button>
          </>
        }
      >
        {check ? <MentionsPanel check={check} docType={draft.docType} customerId={draft.customerId} /> : null}
        {check ? (
          <ArabicGuardBanner
            input={{
              lang: draft.lang,
              secondLang: draft.secondLang,
              productIds: draft.productIds,
              consumer: check.buyerKind === "consumer",
            }}
            onSwitch={onSwitchLang}
          />
        ) : null}
        {invoice ? (
          <>
            <TendersEditor
              withCredit={draft.customerId !== null}
              onChange={setTenders}
              disabled={validate.isPending}
            />
            {settlement.data ? <SettlementFigures settlement={settlement.data} /> : null}
            {settlement.error ? (
              <p role="alert" className="text-danger">
                {errorText(settlement.error)}
              </p>
            ) : null}
          </>
        ) : null}
        {error !== null ? (
          <p role="alert" className="text-danger">
            {errorText(error)}
          </p>
        ) : null}
      </Dialog>
      {approval.dialog}
    </>
  );
}
