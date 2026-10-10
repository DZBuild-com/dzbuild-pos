import { Delete } from "lucide-react";
import { type FormEvent, Suspense, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { LanguageToggle } from "@/components/language-toggle";
import { useApproval } from "@/components/manager-approval";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ArabicGuardBanner } from "@/features/documents/ArabicGuardBanner";
import { isApprovalCancelled } from "@/lib/approval";
import { formatAmountInput, formatNumber } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { toAppError } from "@/lib/ipc/client";
import { useCustomer } from "@/lib/ipc/customers";
import { useHardwareConfig } from "@/lib/ipc/hardware";
import { usePackModules } from "@/lib/ipc/pack-state";
import { useTillPaymentTypes } from "@/lib/ipc/completion";
import { useTicketPreview } from "@/lib/ipc/sales";
import type { Lang } from "@/lib/i18n";
import type { Module, PaymentInput, PaymentMethod, PaymentType, SaleExtras } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty, parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { useCart } from "./cart-store";
import { CreditFigures, CustomerName } from "./CustomerPicker";
import { receiptByDefault, useFinishSale } from "./finish-sale";
import { PaymentTiles, PointsLine } from "./pack-slots";
import {
  QUICK_NOTES,
  type Tender,
  exactAmount,
  needsReference,
  newTender,
  onAccount,
  pressKey,
  restOfTtc,
  toInputs,
} from "./payment-model";
import type { Done } from "./PosDialogs";
import { type TillTender, useTillPayment } from "./till-payment";
import { useCartPreview } from "./use-cart-preview";

const KEYPAD = ["7", "8", "9", "4", "5", "6", "1", "2", "3", "00", "0", ","] as const;
/** M8: with one of these, the voucher tile reads a voucher, a deposit or the customer's store
 * credit (`VoucherTender`), customer or not. */
const VOUCHER_MODULES: Module[] = ["gift_vouchers", "loyalty", "orders", "repairs"];

/** Keypad, notes and tiles never take the focus from the amount being typed. */
const keepFocus = (event: { preventDefault: () => void }) => event.preventDefault();

interface Shortage {
  nameAr: string;
  nameFr: string;
  nameEn: string;
  available: number;
  needed: number;
}

/** A settlement figure, or a dash while the backend has none for the tenders as typed (it refused
 * them, e.g. a cheque without its details, or has not answered yet): never a made-up zero. */
function Figure({ value, className }: { value: number | undefined; className?: string }) {
  const { t } = useTranslation();
  if (value !== undefined) return <Amount value={value} className={className} />;
  return (
    <span className={className}>
      <span aria-hidden>—</span>
      <span className="sr-only">{t("payments:unknown")}</span>
    </span>
  );
}

/** A table ticket paid in place of the till's cart (M8 restaurant, RST-009): its language and
 * customer are the ticket's, and its figures are its bill's (`dining_ticket_preview`). */
export interface TicketPayment {
  id: string;
  lang: Lang;
  customerId: string | null;
}

/** Spec §10.5 payment dialog. Every figure — amount due, duty, rest, change — is the backend's
 * `sales_preview` for the typed tenders; Valider sends the same cart and tenders to `sales_create`
 * (or `returns_exchange` when a return pays part of it). A pack flow that sells through its own
 * command passes `onPay` (a repair's pickup with the tenders to start from; a table ticket, M8
 * restaurant, with `ticket`, whose figures are then the ticket's bill). */
export function PaymentDialog({
  onClose,
  onDone,
  ticket,
  onPay,
  initialTenders,
  initialError,
}: {
  onClose: () => void;
  onDone?: (done: Done) => void;
  ticket?: TicketPayment;
  /** Called by Valider instead of `sales_create`, under the manager's approval when one is asked,
   * with the sale's pack part (serials, register entries, buyer, the tenders' payment types, by
   * the index of each payment in `payments`); a `Done` it resolves with goes to `onDone`. */
  onPay?: (payments: PaymentInput[], printReceipt: boolean, extras: SaleExtras) => Promise<Done | void>;
  /** The tenders the dialog opens with (the last one active). By default one tender of the
   * till's first payment method (Settings › Payments): cash, empty, unless the shop chose one
   * method or narrowed the list, when it opens with the exact amount typed. */
  initialTenders?: Tender[];
  /** What the backend answered when the till's "one method" checkout tried these tenders. */
  initialError?: unknown;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const hardware = useHardwareConfig();
  const tillExchange = useCart((s) => s.exchange);
  const exchange = ticket ? null : tillExchange;
  const tillLang = useCart((s) => s.lang);
  const docLang = ticket?.lang ?? tillLang;
  const lines = useCart((s) => s.lines);
  const productIds = useMemo(
    () => [...new Set(lines.flatMap((d) => (d.line.productId ? [d.line.productId] : [])))],
    [lines],
  );
  const tillCustomer = useCart((s) => s.customerId);
  const customerId = ticket ? ticket.customerId : tillCustomer;
  const customer = useCustomer(customerId);
  const seesCredit = useSession().data?.permissions.includes("credit.view") ?? false;
  // M8: the voucher reader and the custom payment types (a table ticket keeps M1b's tenders)
  const modules = usePackModules();
  const vouchers = !ticket && modules.some((m) => VOUCHER_MODULES.includes(m));
  const payment = useTillPayment();
  // Settings › Payments: the tiles the shop allows; a custom type follows the method it rides
  const methods = payment.allowed;
  const allTypes = useTillPaymentTypes(!ticket);
  const types = useMemo(
    () => allTypes.filter((type) => methods.includes(type.baseMethod)),
    [allTypes, methods],
  );
  const [voucherOpen, setVoucherOpen] = useState(false);
  /** The tender the dialog opens on: the till's one method, else the first allowed method this
   * sale can use (credit and store credit need a customer; the voucher reader is not a tender). */
  const start = (): TillTender => {
    if (payment.fixed) return ticket ? { method: payment.fixed.method, type: null } : payment.fixed;
    const usable = methods.find(
      (m) => !(onAccount(m) && customerId === null) && !(m === "voucher" && vouchers),
    );
    return { method: usable ?? methods[0] ?? "cash", type: null };
  };
  const startTender = ({ method, type }: TillTender, amount: string): Tender => {
    const tender = newTender(method, amount);
    return type ? { ...tender, type: { id: type.id, name: localizedName(type, lang) } } : tender;
  };
  // UX-004: the receipt starts in the language of the customer's file, when it has one
  const customerLang = ticket ? null : (customer.data?.lang ?? null);
  useEffect(() => {
    if (customerLang) useCart.getState().setLang(customerLang);
  }, [customerLang]);
  const [tenders, setTenders] = useState<Tender[]>(
    () => initialTenders ?? [startTender(payment.known ? start() : { method: "cash", type: null }, "")],
  );
  const [active, setActive] = useState(() => Math.max(0, (initialTenders?.length ?? 1) - 1));
  // the opening tender gets its method and exact amount once the settings and the figures are in
  const [seeded, setSeeded] = useState(initialTenders !== undefined);
  // the exact amount was typed for the cashier: the first key or note replaces it
  const [fresh, setFresh] = useState(false);
  const configured = Boolean(hardware.data?.receiptPrinterId);
  // The receipt's second language, when it is not the one chosen for this sale.
  const configuredSecond = hardware.data?.receipt?.secondLang ?? null;
  const receiptSecond = configuredSecond !== docLang ? configuredSecond : null;
  // the shop's default until the cashier ticks or unticks it (SAL-052), even when the
  // hardware settings arrive after the dialog opened
  const [printChoice, setPrintChoice] = useState<boolean | null>(null);
  const printReceipt = printChoice === null ? receiptByDefault(hardware.data) : configured && printChoice;
  const [error, setError] = useState<unknown>(initialError ?? null);
  const inputs = useMemo(() => toInputs(tenders), [tenders]);
  const tillPreview = useCartPreview(inputs ?? [], exchange?.value ?? null, ticket === undefined);
  const ticketPreview = useTicketPreview(ticket?.id ?? null, inputs ?? []);
  const preview = ticket ? ticketPreview : tillPreview;
  const settlement = preview.data?.settlement;
  const approval = useApproval();
  const finishSale = useFinishSale(approval.run);
  const busy = finishSale.busy;
  const ready =
    inputs !== null &&
    settlement !== undefined &&
    settlement.missing === 0 &&
    !preview.error &&
    !preview.isFetching &&
    !busy;

  const setAmount = (index: number, amount: string) => {
    setFresh(false);
    setTenders((all) => all.map((t, i) => (i === index ? { ...t, amount } : t)));
  };
  const update = (index: number, change: Partial<Tender>) =>
    setTenders((all) => all.map((t, i) => (i === index ? { ...t, ...change } : t)));
  const cashIndex = () => {
    const found = tenders.findIndex((t) => t.method === "cash");
    if (found >= 0) return found;
    setTenders((all) => [...all, newTender("cash")]);
    return tenders.length;
  };
  const typed = (index: number) => parseAmount(tenders[index]?.amount ?? "") ?? 0;

  // the rest of the TTC after what is already typed: card payments give no change (PAY-017);
  // nothing is proposed while the backend has no settlement for these tenders (an older one
  // would count a tender twice)
  const rest = settlement ? restOfTtc(settlement) : 0;
  /** What a new tender of `method` is proposed with: the rest, and store credit up to what the
   * customer has (PAY-011; the backend checks it again). */
  const proposedFor = (method: PaymentMethod) =>
    method === "voucher" ? Math.min(rest, customer.data?.storeCredit ?? 0) : rest;
  const add = (tender: Tender) => {
    setFresh(false);
    setTenders((all) => [...all, tender]);
    setActive(tenders.length);
  };

  // The opening tender gets its method and its exact amount once the settings and the figures are
  // in: adjusted while rendering, as React does for state that follows new data.
  const opening = payment.known ? start() : null;
  // store credit is proposed up to the customer's: his file is waited for
  const waitsForCustomer =
    payment.prefill && opening?.method === "voucher" && customerId !== null && !customer.data;
  if (!seeded && opening && settlement && !preview.isFetching && !preview.error && !waitsForCustomer) {
    setSeeded(true);
    // only an untouched opening tender is replaced
    const untouched = tenders.length === 1 && tenders[0]?.amount === "";
    if (untouched && !payment.prefill && tenders[0]?.method !== opening.method) {
      setTenders([startTender(opening, "")]);
    } else if (untouched && payment.prefill) {
      const blocked = onAccount(opening.method) && customerId === null;
      // with no tender typed the backend counts the whole amount as cash: `missing` is the exact
      // cash amount, the duty and the rounding included
      const amount = blocked
        ? 0
        : opening.method === "cash"
          ? settlement.missing
          : proposedFor(opening.method);
      setTenders([startTender(opening, amount > 0 ? formatAmountInput(amount, lang) : "")]);
      setActive(0);
      setFresh(amount > 0);
    }
  }

  // a typed exact amount is selected, so that typing the note handed over replaces it
  useEffect(() => {
    if (!fresh) return;
    const input = document.getElementById("tender-0");
    if (input instanceof HTMLInputElement) input.select();
  }, [fresh]);
  /** A tender of `method`, or of a custom payment type riding it (core Task 16). */
  const pick = (method: PaymentMethod, type?: PaymentType) => {
    if (method === "cash") {
      setActive(cashIndex());
      return;
    }
    const proposed = proposedFor(method);
    const tender = newTender(method, proposed > 0 ? formatAmountInput(proposed, lang) : "");
    add(type ? { ...tender, type: { id: type.id, name: localizedName(type, lang) } } : tender);
  };
  const key = (k: (typeof KEYPAD)[number]) =>
    setAmount(active, pressKey(fresh ? "" : (tenders[active]?.amount ?? ""), k));
  // the notes are cash: a till that takes no cash has none
  const takesCash = methods.includes("cash");
  const quick = (note: number) => {
    const index = cashIndex();
    setAmount(index, formatAmountInput((fresh && index === 0 ? 0 : typed(index)) + note, lang));
    setActive(index);
  };
  /** Cash takes what is missing; at a till without cash, the tender being typed takes the rest. */
  const exact = () => {
    const index = takesCash ? cashIndex() : active;
    const method = tenders[index]?.method ?? "cash";
    if (settlement) {
      setAmount(index, formatAmountInput(typed(index) + exactAmount(method, settlement), lang));
    }
    setActive(index);
  };

  const submit = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!ready || inputs === null) return;
    setError(null);
    try {
      const done = await finishSale.finish({ tenders, inputs, printReceipt, exchange, onPay });
      if (done) onDone?.(done);
    } catch (e) {
      if (!isApprovalCancelled(e)) setError(e);
    }
  };

  const shortages =
    (toAppError(error).details as { shortages?: Shortage[] } | null | undefined)?.shortages ?? [];
  // Spec §10.5 at 1024×768: the rest to pay (or the change) and Valider stay in the footer, always
  // in view; the tenders and the keypad scroll if they must.
  const band =
    settlement === undefined ? (
      <div
        role="status"
        aria-label={t("payments:missing")}
        className="flex min-h-16 flex-1 items-center justify-between gap-3 rounded-control bg-surface-2 px-4 text-ink-2"
      >
        <span className="text-lg font-semibold">{t("payments:missing")}</span>
        <Figure value={undefined} className="text-3xl font-bold" />
      </div>
    ) : settlement.change > 0 || settlement.missing === 0 ? (
      <div
        role="status"
        aria-label={t("payments:change")}
        className="flex min-h-16 flex-1 items-center justify-between gap-3 rounded-control bg-ok-soft px-4 text-ok"
      >
        <span className="text-lg font-semibold">{t("payments:change")}</span>
        <Amount value={settlement.change} className="text-3xl font-bold" />
      </div>
    ) : (
      <div
        role="status"
        aria-label={t("payments:missing")}
        className="flex min-h-16 flex-1 items-center justify-between gap-3 rounded-control bg-warn-soft px-4 text-warn"
      >
        <span className="text-lg font-semibold">{t("payments:missing")}</span>
        <Amount value={settlement.missing} className="text-3xl font-bold" />
      </div>
    );
  return (
    <>
      <Dialog
        open={!approval.pending}
        size="xl"
        onOpenChange={(open) => (open ? undefined : onClose())}
        title={t("payments:title")}
        footer={
          <div className="flex w-full flex-wrap items-stretch gap-2">
            {band}
            <Button variant="outline" size="lg" className="min-h-16" onClick={onClose}>
              {t("common:actions.cancel")}
            </Button>
            <Button
              variant="primary"
              size="lg"
              className="min-h-16 min-w-56 text-xl"
              disabled={!ready}
              onClick={() => void submit()}
              shortcut="Enter"
              shortcutLabel={t("common:keys.enter")}
            >
              {t("payments:validate")}
            </Button>
          </div>
        }
      >
        <form
          noValidate
          onSubmit={(e) => void submit(e)}
          onKeyDown={(e) => {
            // Enter validates from any field (spec §10.5); a focused button keeps its own Enter
            if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) {
              e.preventDefault();
              void submit();
            }
          }}
          className="grid gap-4 md:grid-cols-2"
        >
          <div className="flex flex-col gap-3">
            {exchange ? (
              <p className="rounded-control bg-info-soft p-3 font-semibold text-info">
                {t("payments:exchangeCredit")} <Amount value={exchange.value} />
              </p>
            ) : null}
            {customer.data ? (
              <div className="flex flex-col gap-1 rounded-control bg-surface-2 p-3">
                <CustomerName customer={customer.data} />
                {seesCredit ? <CreditFigures customer={customer.data} /> : null}
              </div>
            ) : null}
            <div className="grid grid-cols-4 gap-2">
              {methods.map((method) => {
                // M8: the voucher tile reads a voucher or a deposit, customer or not
                const reader = method === "voucher" && vouchers;
                const blocked = onAccount(method) && customerId === null && !reader;
                return (
                  <button
                    key={method}
                    type="button"
                    disabled={blocked}
                    aria-describedby={blocked ? "need-customer" : undefined}
                    onMouseDown={keepFocus}
                    onClick={() => (reader ? setVoucherOpen(true) : pick(method))}
                    className={cn(
                      "min-h-14 rounded-control border px-2 font-semibold disabled:opacity-50",
                      tenders[active]?.method === method
                        ? "border-brand bg-brand-soft"
                        : "border-line bg-surface-2",
                    )}
                  >
                    {reader ? t("payments:voucherTile") : t(`payments:tenders.${method}`)}
                  </button>
                );
              })}
            </div>
            {/* M8: the custom payment types' tiles, the voucher reader and the BaridPay QR */}
            {!ticket && (types.length > 0 || voucherOpen || tenders[active]?.method === "baridimob") ? (
              <Suspense fallback={null}>
                <PaymentTiles
                  types={types}
                  voucherOpen={voucherOpen}
                  onVoucherClose={() => setVoucherOpen(false)}
                  customerId={customerId}
                  storeCredit={customer.data?.storeCredit ?? 0}
                  rest={settlement ? rest : null}
                  method={tenders[active]?.method}
                  onType={(type) => pick(type.baseMethod, type)}
                  onTender={add}
                />
              </Suspense>
            ) : null}
            {customerId === null ? (
              <p id="need-customer" className="text-sm text-ink-2">
                {t("payments:needCustomer")}
              </p>
            ) : null}
            <ul className="flex flex-col gap-3">
              {tenders.map((tender, index) => (
                <li
                  key={index}
                  className={cn(
                    "flex flex-col gap-2 rounded-card border p-3",
                    index === active ? "border-brand" : "border-line",
                  )}
                >
                  <Field
                    id={`tender-${index}`}
                    label={
                      tender.type?.name ??
                      (tender.method === "voucher" && vouchers
                        ? t("payments:voucherTile")
                        : t(`payments:tenders.${tender.method}`))
                    }
                  >
                    <NumericInput
                      id={`tender-${index}`}
                      // the dialog's own keypad types the amounts: no on-screen keyboard over it
                      data-osk="off"
                      autoFocus={index === active}
                      value={tender.amount}
                      onFocus={() => setActive(index)}
                      onValueChange={(v) => setAmount(index, v)}
                    />
                  </Field>
                  {needsReference(tender.method) ? (
                    <Field
                      id={`tender-ref-${index}`}
                      label={
                        tender.type
                          ? t("payments:typeReference")
                          : t(`payments:reference.${tender.method}`, {
                              method: t(`payments:tenders.${tender.method}`),
                            })
                      }
                    >
                      <GuardedInput
                        id={`tender-ref-${index}`}
                        dir="ltr"
                        value={tender.reference}
                        maxLength={40}
                        onValueChange={(reference) => update(index, { reference })}
                      />
                    </Field>
                  ) : null}
                  {tender.method === "cheque" ? (
                    <div className="grid grid-cols-2 gap-2">
                      <Field id={`cheque-bank-${index}`} label={t("payments:cheque.bank")}>
                        <GuardedInput
                          id={`cheque-bank-${index}`}
                          value={tender.chequeBank}
                          maxLength={48}
                          onValueChange={(chequeBank) => update(index, { chequeBank })}
                        />
                      </Field>
                      <Field id={`cheque-date-${index}`} label={t("payments:cheque.date")}>
                        <Input
                          id={`cheque-date-${index}`}
                          type="date"
                          dir="ltr"
                          value={tender.chequeDate}
                          onChange={(e) => update(index, { chequeDate: e.target.value })}
                        />
                      </Field>
                    </div>
                  ) : null}
                  {tenders.length > 1 ? (
                    <Button
                      variant="ghost"
                      onClick={() => {
                        setTenders((all) => all.filter((_, i) => i !== index));
                        setActive(0);
                      }}
                    >
                      {t("common:actions.remove")}
                    </Button>
                  ) : null}
                </li>
              ))}
            </ul>
            <div className="grid grid-cols-3 gap-2">
              {(takesCash ? QUICK_NOTES : []).map((note) => (
                <Button
                  key={note}
                  variant="outline"
                  className="min-h-14 px-2"
                  onMouseDown={keepFocus}
                  onClick={() => quick(note)}
                >
                  {t("payments:quickNote", { value: formatNumber(note / 100, lang) })}
                </Button>
              ))}
              <Button variant="secondary" className="min-h-14 px-2" onMouseDown={keepFocus} onClick={exact}>
                {t("payments:exact")}
              </Button>
            </div>
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
              {/* a table ticket prints in the language it was opened in */}
              {ticket ? null : (
                <div role="group" aria-labelledby="pay-lang" className="flex items-center gap-x-4">
                  <span id="pay-lang" className="font-semibold">
                    {t("payments:printLang")}
                  </span>
                  <LanguageToggle value={docLang} onChange={(l) => useCart.getState().setLang(l)} />
                </div>
              )}
              <label className="flex items-center gap-3 font-semibold">
                <Checkbox
                  checked={printReceipt}
                  disabled={!configured}
                  onCheckedChange={(v) => setPrintChoice(v === true)}
                />
                {t("payments:printReceipt")}
              </label>
            </div>
            {/* DOC-034: a ticket is a consumer document; the suggestion never blocks the sale. A
                bilingual receipt (Réglages › Matériel) prints its second language too. */}
            {ticket ? null : (
              <ArabicGuardBanner
                input={{ lang: docLang, secondLang: receiptSecond, productIds, consumer: true }}
                onSwitch={(l) => useCart.getState().setLang(l)}
              />
            )}
            {!configured ? <p className="text-sm text-ink-2">{t("payments:noPrinter")}</p> : null}
          </div>
          <div className="flex flex-col gap-3">
            <div className="grid grid-cols-3 gap-2" dir="ltr">
              {KEYPAD.map((k) => (
                <button
                  key={k}
                  type="button"
                  tabIndex={-1}
                  aria-label={k}
                  onMouseDown={keepFocus}
                  onClick={() => key(k)}
                  className="grid min-h-14 place-items-center rounded-control bg-surface-2 text-2xl font-bold hover:bg-line"
                >
                  {k}
                </button>
              ))}
              <button
                type="button"
                tabIndex={-1}
                aria-label={t("auth:pinPad.erase")}
                onMouseDown={keepFocus}
                onClick={() => setAmount(active, (tenders[active]?.amount ?? "").slice(0, -1))}
                className="col-span-3 grid min-h-14 place-items-center rounded-control bg-surface-2 hover:bg-line"
              >
                <Delete aria-hidden />
              </button>
            </div>
            <dl className="grid gap-1">
              <div className="flex justify-between">
                <dt>{t("payments:totalTtc")}</dt>
                <dd>
                  <Figure value={settlement?.totalTtc} />
                </dd>
              </div>
              {settlement && settlement.stampCharged !== 0 ? (
                <div className="flex justify-between">
                  <dt>{t("payments:stampDuty")}</dt>
                  <dd>
                    <Amount value={settlement.stampCharged} />
                  </dd>
                </div>
              ) : null}
              <div className="flex justify-between font-bold">
                <dt>{t("payments:totalDue")}</dt>
                <dd>
                  <Figure value={settlement?.totalDue} />
                </dd>
              </div>
            </dl>
            {/* M8 (LOY-002): the points the ticket earns an enrolled customer, the backend's */}
            {!ticket && customerId !== null && modules.includes("loyalty") ? (
              <Suspense fallback={null}>
                <PointsLine customerId={customerId} totalTtc={settlement?.totalTtc} payments={inputs ?? []} />
              </Suspense>
            ) : null}
            {settlement && settlement.stampCharged !== 0 ? (
              <p className="rounded-control bg-info-soft p-2 text-sm text-info">
                {t("payments:stampHint")}
                <Amount value={settlement.stampCharged} />
              </p>
            ) : null}
            {preview.error ? (
              <p role="alert" className="text-danger">
                {errorText(preview.error)}
              </p>
            ) : null}
            {error ? (
              <div role="alert" className="flex flex-col gap-1 text-danger">
                <p>{errorText(error)}</p>
                {shortages.map((s) => (
                  <p key={s.nameFr + s.needed}>
                    {t("payments:shortage", {
                      name: localizedName(s, lang),
                      available: formatQty(s.available, lang),
                      needed: formatQty(s.needed, lang),
                    })}
                  </p>
                ))}
              </div>
            ) : null}
          </div>
        </form>
      </Dialog>
      {approval.dialog}
    </>
  );
}
