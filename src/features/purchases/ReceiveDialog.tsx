import { ArrowRight, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formatAmountInput } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { toAppError } from "@/lib/ipc/client";
import { useReceive, useRemaining } from "@/lib/ipc/purchases";
import type { PurchaseLineInput, ReceiptDone, RemainingLine, Supplier } from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { InvoiceDialog } from "./InvoiceDialog";
import {
  GridEntryButton,
  type LineDraft,
  type LineErrors,
  ProductPicker,
  SerialList,
  emptyLine,
  isNothing,
  refusedLine,
  toLineInput,
  useLineLabel,
  useTracking,
  useUnitDecimals,
} from "./purchase-lines";

type Lang = ReturnType<typeof useLang>;

/** An order line still awaited: its remaining quantity and its ordered cost per unit. */
function orderLine(r: RemainingLine, lang: Lang): LineDraft {
  return {
    key: r.line.id,
    productId: r.line.productId ?? "",
    name: r.line.description,
    unitCode: r.line.unitCode,
    qty: formatQty(r.remaining, lang),
    pack: "",
    cost: formatAmountInput(r.line.unitPrice, lang),
    originLineId: r.line.id,
    lotNumber: "",
    expiryDate: "",
    serials: [],
  };
}

/**
 * PUR-003/004 (goods receipt): against a validated order (its remaining lines, quantities and costs
 * prefilled and editable; 0 skips a line) or without one (lines by scan or search). A lot-tracked
 * product takes its lot number and expiry date, a serial-tracked one a serial number per piece.
 * The receipt moves the stock and the CMUP in one backend transaction; the CMUP before and after
 * each line comes back from the backend (shown with `reports.profit`). The supplier's invoice can
 * be recorded with it, or right after.
 */
export function ReceiveDialog({
  supplier,
  orderId = null,
  open,
  onClose,
}: {
  supplier: Supplier;
  orderId?: string | null;
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const label = useLineLabel();
  const decimalsOf = useUnitDecimals();
  const seesCosts = useSession().data?.permissions.includes("reports.profit") ?? false;
  const remaining = useRemaining(open ? orderId : null);
  const [rows, setRows] = useState<LineDraft[]>([]);
  const [supplierRef, setSupplierRef] = useState("");
  const [note, setNote] = useState("");
  const [invoiceNow, setInvoiceNow] = useState(false);
  const [invoiceNumber, setInvoiceNumber] = useState("");
  const [errors, setErrors] = useState<Record<string, LineErrors>>({});
  const [problem, setProblem] = useState<string | null>(null);
  const [sentKeys, setSentKeys] = useState<string[]>([]);
  // The names of the lines sent: a line fully received leaves the order's list afterwards.
  const [sentNames, setSentNames] = useState<Record<string, string>>({});
  const [done, setDone] = useState<ReceiptDone | null>(null);
  const [invoicing, setInvoicing] = useState(false);
  const receive = useReceive();

  // The order's awaited lines, with what was typed on them.
  const fromOrder = (current: LineDraft[]): LineDraft[] =>
    (remaining.data ?? [])
      .filter((r) => r.remaining > 0 && r.line.productId !== null && r.line.parentLineId === null)
      .map((r) => current.find((row) => row.originLineId === r.line.id) ?? orderLine(r, lang));
  const lines = orderId ? fromOrder(rows) : rows;
  const tracking = useTracking(lines.map((l) => l.productId));
  const patch = (key: string, change: Partial<LineDraft>) => {
    setRows((current) =>
      (orderId ? fromOrder(current) : current).map((l) => (l.key === key ? { ...l, ...change } : l)),
    );
    setErrors((all) => ({ ...all, [key]: {} }));
  };

  const refusal = receive.error ? toAppError(receive.error) : null;
  const refusedAt = refusedLine(refusal?.field);
  const refusedKey = refusedAt !== null ? sentKeys[refusedAt] : undefined;

  const submit = () => {
    const found: Record<string, LineErrors> = {};
    const sent: PurchaseLineInput[] = [];
    const keys: string[] = [];
    for (const line of lines) {
      if (isNothing(line.qty)) continue;
      const result = toLineInput(line, decimalsOf(line.unitCode), tracking.of(line.productId));
      if (result.input) {
        sent.push(result.input);
        keys.push(line.key);
      } else {
        found[line.key] = result.errors;
      }
    }
    let general: string | null = null;
    if (sent.length === 0 && Object.keys(found).length === 0) general = t("purchases:receive.nothing");
    if (invoiceNow && invoiceNumber.trim() === "") general = t("purchases:receive.invoiceNumberMissing");
    setErrors(found);
    setProblem(general);
    if (general !== null || Object.keys(found).length > 0) return;
    setSentKeys(keys);
    setSentNames(Object.fromEntries(lines.map((l) => [l.productId, l.name])));
    receive.mutate(
      {
        supplierId: supplier.id,
        orderId,
        locationId: null,
        lines: sent,
        supplierRef: supplierRef.trim() === "" ? null : supplierRef.trim(),
        note: note.trim() === "" ? null : note.trim(),
        invoice: invoiceNow ? { supplierNumber: invoiceNumber.trim(), payment: null } : null,
      },
      {
        onSuccess: (result) => {
          toast.success(t("purchases:receive.done", { number: result.receipt.number ?? "" }));
          setDone(result);
        },
      },
    );
  };

  if (done && invoicing) {
    return <InvoiceDialog supplier={supplier} receiptIds={[done.receipt.id]} open={open} onClose={onClose} />;
  }
  if (done) {
    return (
      <Dialog
        open={open}
        onOpenChange={(o) => (o ? undefined : onClose())}
        title={t("purchases:receive.doneTitle", { number: done.receipt.number ?? "" })}
        footer={
          <>
            {done.invoice ? null : (
              <Button variant="secondary" onClick={() => setInvoicing(true)}>
                {t("purchases:receive.recordInvoice")}
              </Button>
            )}
            <Button variant="primary" onClick={onClose}>
              {t("common:actions.done")}
            </Button>
          </>
        }
      >
        {done.invoice ? (
          <p className="font-semibold">
            {t("purchases:receive.invoiced", { number: done.invoice.number ?? "" })}
          </p>
        ) : null}
        {seesCosts && done.costChanges.length > 0 ? (
          <section className="flex flex-col gap-2" aria-labelledby="receive-cmup">
            <h3 id="receive-cmup" className="text-lg font-bold">
              {t("purchases:receive.cmupTitle")}
            </h3>
            <ul className="flex flex-col gap-2">
              {done.costChanges.map((c, i) => (
                <li
                  key={`${c.productId}-${i}`}
                  className="flex flex-wrap items-center justify-between gap-2 rounded-control bg-surface-2 p-3"
                >
                  <span className="font-semibold">{sentNames[c.productId]}</span>
                  <span className="flex items-center gap-2">
                    {t("purchases:receive.cmup")}
                    <Amount value={c.costBefore} />
                    <ArrowRight aria-hidden className="size-4 rtl:-scale-x-100" />
                    <span className="sr-only">{t("purchases:receive.becomes")}</span>
                    <Amount value={c.costAfter} className="font-bold" />
                  </span>
                </li>
              ))}
            </ul>
          </section>
        ) : null}
      </Dialog>
    );
  }

  const busy = receive.isPending || (orderId !== null && remaining.isPending) || !tracking.ready;
  return (
    <Dialog
      open={open}
      size="xl"
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("purchases:receive.title", { name: supplier.name })}
      description={orderId ? t("purchases:receive.fromOrder") : t("purchases:receive.free")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={busy} onClick={submit}>
            {t("purchases:receive.confirm")}
          </Button>
        </>
      }
    >
      <form
        noValidate
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        {orderId ? null : (
          <>
            <ProductPicker
              autoFocus
              scanAnywhere
              onPick={(p) =>
                setRows((all) => [...all, emptyLine(p, p.packQty === null ? "" : formatQty(p.packQty, lang))])
              }
            />
            <GridEntryButton
              onAdd={(entered) =>
                setRows((all) => [
                  ...all,
                  ...entered.map((line) => ({
                    ...emptyLine(
                      {
                        productId: line.variantId,
                        name: line.name,
                        unitCode: line.unitCode,
                        packQty: null,
                      },
                      "",
                    ),
                    qty: formatQty(line.qty, lang),
                  })),
                ])
              }
            />
          </>
        )}
        {remaining.error ? (
          <p role="alert" className="text-danger">
            {errorText(remaining.error)}
          </p>
        ) : null}
        {orderId && remaining.isSuccess && lines.length === 0 ? (
          <p className="text-ink-2">{t("purchases:receive.allReceived")}</p>
        ) : null}
        {orderId ? <p className="text-sm text-ink-2">{t("purchases:receive.skipHint")}</p> : null}
        <ul className="flex flex-col gap-3">
          {lines.map((line) => {
            const info = tracking.of(line.productId);
            const lineErrors = errors[line.key] ?? {};
            const awaited = remaining.data?.find((r) => r.line.id === line.originLineId)?.remaining;
            const err = (field: keyof LineErrors) => {
              const key = lineErrors[field];
              if (!key) return undefined;
              return t(key, { count: line.serials.length, qty: line.qty });
            };
            return (
              <li key={line.key} className="flex flex-col gap-3 rounded-card border border-line p-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <p className="font-semibold">
                    {line.name}
                    {awaited !== undefined ? (
                      <span className="ms-2 text-sm font-normal text-ink-2">
                        {t("purchases:receive.awaited", { qty: formatQty(awaited, lang) })}
                      </span>
                    ) : null}
                  </p>
                  {orderId ? null : (
                    <Button
                      variant="ghost"
                      size="icon"
                      aria-label={t("purchases:lines.removeOf", { name: line.name })}
                      onClick={() => setRows((all) => all.filter((l) => l.key !== line.key))}
                    >
                      <X aria-hidden />
                    </Button>
                  )}
                </div>
                <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
                  <Field id={`receive-qty-${line.key}`} label={t("purchases:receive.qty")} error={err("qty")}>
                    <NumericInput
                      id={`receive-qty-${line.key}`}
                      kind="qty"
                      aria-label={label("purchases:receive.qty", line.name)}
                      aria-invalid={Boolean(lineErrors.qty)}
                      value={line.qty}
                      onValueChange={(qty) => patch(line.key, { qty })}
                    />
                  </Field>
                  {line.pack !== "" ? (
                    <Field
                      id={`receive-pack-${line.key}`}
                      label={t("purchases:lines.pack")}
                      error={err("pack")}
                    >
                      <NumericInput
                        id={`receive-pack-${line.key}`}
                        kind="qty"
                        aria-label={label("purchases:lines.pack", line.name)}
                        value={line.pack}
                        onValueChange={(pack) => patch(line.key, { pack })}
                      />
                    </Field>
                  ) : null}
                  <Field
                    id={`receive-cost-${line.key}`}
                    label={t(line.pack !== "" ? "purchases:lines.packCost" : "purchases:receive.cost")}
                    error={err("cost")}
                  >
                    <NumericInput
                      id={`receive-cost-${line.key}`}
                      aria-label={label(
                        line.pack !== "" ? "purchases:lines.packCost" : "purchases:receive.cost",
                        line.name,
                      )}
                      aria-invalid={Boolean(lineErrors.cost)}
                      value={line.cost}
                      onValueChange={(cost) => patch(line.key, { cost })}
                    />
                  </Field>
                  {info?.lots ? (
                    <>
                      <Field
                        id={`receive-lot-${line.key}`}
                        label={t("purchases:receive.lot")}
                        error={err("lotNumber")}
                      >
                        <GuardedInput
                          id={`receive-lot-${line.key}`}
                          dir="ltr"
                          maxLength={40}
                          aria-label={label("purchases:receive.lot", line.name)}
                          aria-invalid={Boolean(lineErrors.lotNumber)}
                          value={line.lotNumber}
                          onValueChange={(lotNumber) => patch(line.key, { lotNumber })}
                        />
                      </Field>
                      <Field id={`receive-expiry-${line.key}`} label={t("purchases:receive.expiry")}>
                        <Input
                          id={`receive-expiry-${line.key}`}
                          type="date"
                          dir="ltr"
                          aria-label={label("purchases:receive.expiry", line.name)}
                          value={line.expiryDate}
                          onChange={(event) => patch(line.key, { expiryDate: event.target.value })}
                        />
                      </Field>
                    </>
                  ) : null}
                </div>
                {info?.serials ? (
                  <SerialList
                    id={`receive-serials-${line.key}`}
                    label={label("purchases:receive.serials", line.name)}
                    serials={line.serials}
                    onChange={(serials) => patch(line.key, { serials })}
                    error={err("serials")}
                  />
                ) : null}
                {refusedKey === line.key ? (
                  <p role="alert" className="text-sm text-danger">
                    {errorText(receive.error)}
                  </p>
                ) : null}
              </li>
            );
          })}
        </ul>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field
            id="receive-ref"
            label={t("purchases:receive.supplierRef")}
            optional={t("customers:form.optional")}
          >
            <GuardedInput
              id="receive-ref"
              dir="ltr"
              maxLength={48}
              value={supplierRef}
              onValueChange={setSupplierRef}
            />
          </Field>
          <Field
            id="receive-note"
            label={t("purchases:receive.note")}
            optional={t("customers:form.optional")}
          >
            <Input
              id="receive-note"
              dir="auto"
              maxLength={500}
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </Field>
        </div>
        <label className="flex min-h-11 items-center gap-3 font-semibold">
          <Checkbox checked={invoiceNow} onCheckedChange={(v) => setInvoiceNow(v === true)} />
          {t("purchases:receive.invoiceNow")}
        </label>
        {invoiceNow ? (
          <Field
            id="receive-invoice-number"
            label={t("purchases:invoice.number")}
            error={refusal?.field === "supplierNumber" ? errorText(receive.error) : undefined}
          >
            <GuardedInput
              id="receive-invoice-number"
              dir="ltr"
              maxLength={48}
              value={invoiceNumber}
              onValueChange={setInvoiceNumber}
            />
          </Field>
        ) : null}
        {problem ? (
          <p role="alert" className="text-danger">
            {problem}
          </p>
        ) : null}
        {receive.error && refusedKey === undefined && refusal?.field !== "supplierNumber" ? (
          <p role="alert" className="text-danger">
            {errorText(receive.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
