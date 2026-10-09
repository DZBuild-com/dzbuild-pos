import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { type Details, MethodDetails, MethodPicker } from "@/features/customers/VersementDialog";
import { CustomerField } from "@/features/documents/editor/CustomerField";
import { formatAmountInput, formatDZD, formatNumber } from "@/lib/format";
import { useSellVoucher, useVoucherQuote } from "@/lib/ipc/loyalty";
import type { PaymentMethod, SellInput, VoucherDone } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { VoucherIssued } from "./VoucherIssued";

/** The quick amounts of a gift voucher, in centimes: 1 000, 2 000 and 5 000 DA. */
const QUICK_AMOUNTS = [100_000, 200_000, 500_000] as const;

const text = (value: string) => (value.trim() === "" ? null : value.trim());

/**
 * LOY-001, PAY-010 lite: a gift voucher sold at the counter (till menu, Ctrl+3). The droit de
 * timbre on its cash and what to collect come from `vouchers_quote_sale` as the cashier types
 * (the UI never computes them); "Encaisser et imprimer" sells it (`BA…`, numbered by the main PC
 * on a till), its slip prints here and the drawer opens for its cash.
 */
export function SellVoucherDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [amount, setAmount] = useState("");
  const [method, setMethod] = useState<PaymentMethod>("cash");
  const [details, setDetails] = useState<Details>({ reference: "", chequeBank: "", chequeDate: "" });
  const [buyer, setBuyer] = useState("");
  const [customerId, setCustomerId] = useState<string | null>(null);
  const [done, setDone] = useState<VoucherDone | null>(null);
  const sell = useSellVoucher();

  const value = parseAmount(amount);
  const { reference, chequeBank, chequeDate } = details;
  // The buyer changes nothing in the figures: typing his name asks the backend nothing.
  const input = useMemo<SellInput | null>(
    () =>
      value !== null && value > 0
        ? {
            amount: value,
            method,
            reference: method === "cash" ? null : text(reference),
            chequeBank: method === "cheque" ? text(chequeBank) : null,
            chequeDate: method === "cheque" ? text(chequeDate) : null,
            customerId: null,
            buyerName: null,
          }
        : null,
    [value, method, reference, chequeBank, chequeDate],
  );
  const asked = useDebounced(input, 250);
  const quote = useVoucherQuote(asked);
  const figures = input !== null && asked === input ? quote.data : undefined;
  const ready = figures !== undefined && !quote.isFetching && !quote.error;

  const submit = () => {
    if (!input || !ready) return;
    sell.mutate({ ...input, customerId, buyerName: text(buyer) }, { onSuccess: setDone });
  };
  const problem = sell.error ?? (input ? quote.error : null);
  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("loyalty:sell.title")}
      footer={
        done ? (
          <Button variant="primary" onClick={onClose}>
            {t("common:actions.done")}
          </Button>
        ) : (
          <>
            <Button variant="outline" onClick={onClose}>
              {t("common:actions.cancel")}
            </Button>
            <Button variant="primary" disabled={!ready || sell.isPending} onClick={submit}>
              {t("loyalty:sell.submit")}
            </Button>
          </>
        )
      }
    >
      {done ? (
        <VoucherIssued done={done} messageKey="loyalty:sell.done" notPrintedKey="loyalty:sell.notPrinted" />
      ) : (
        <form
          noValidate
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <div className="flex flex-wrap gap-2">
            {QUICK_AMOUNTS.map((note) => (
              <Button
                key={note}
                variant="secondary"
                size="lg"
                aria-pressed={value === note}
                onClick={() => setAmount(formatAmountInput(note, lang))}
              >
                {t("payments:quickNote", { value: formatNumber(note / 100, lang) })}
              </Button>
            ))}
          </div>
          <Field id="voucher-amount" label={t("loyalty:sell.amount")}>
            <NumericInput
              id="voucher-amount"
              autoFocus
              value={amount}
              onValueChange={setAmount}
              className="text-2xl font-bold"
            />
          </Field>
          <MethodPicker value={method} onChange={setMethod} />
          <MethodDetails method={method} value={details} onChange={setDetails} idPrefix="voucher" />
          <div className="grid gap-4 sm:grid-cols-2">
            <Field id="voucher-buyer" label={t("loyalty:sell.buyer")}>
              <Input
                id="voucher-buyer"
                dir="auto"
                maxLength={120}
                value={buyer}
                onChange={(event) => setBuyer(event.target.value)}
              />
            </Field>
            <CustomerField value={customerId} onChange={setCustomerId} />
          </div>
          {figures ? (
            <p aria-live="polite" className="rounded-card bg-surface-2 p-4 text-xl font-bold">
              {figures.stampCharged > 0
                ? t("loyalty:sell.toReceive", {
                    amount: formatDZD(figures.toReceive, lang),
                    duty: formatDZD(figures.stampCharged, lang),
                  })
                : t("loyalty:sell.receive", { amount: formatDZD(figures.toReceive, lang) })}
            </p>
          ) : null}
          {problem ? (
            <p role="alert" className="text-danger">
              {errorText(problem)}
            </p>
          ) : null}
        </form>
      )}
    </Dialog>
  );
}
