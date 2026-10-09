import { type FormEvent, Suspense, useEffect, useEffectEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { ScanInput } from "@/components/scan-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { isApprovalCancelled } from "@/lib/approval";
import { formatAmountInput } from "@/lib/format";
import { useCustomer } from "@/lib/ipc/customers";
import { useFindDocument } from "@/lib/ipc/documents";
import { useHardwareConfig } from "@/lib/ipc/hardware";
import { usePackModules } from "@/lib/ipc/pack-state";
import { useProductSearch } from "@/lib/ipc/products";
import { useCreateReturn, useReturnQuote, useReturnable } from "@/lib/ipc/returns";
import type {
  Document,
  PaymentMethod,
  ReturnableLine,
  ReturnExtras,
  ReturnInput,
  ReturnQuote,
  ReturnReason,
  ReturnSerial,
} from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty, normalizeDigits, parseAmount, parseQty } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { useCart } from "./cart-store";
import { ReturnSerials } from "./pack-slots";
import { needsReference } from "./payment-model";
import type { PosContext } from "./PosDialogs";

const REASONS: ReturnReason[] = ["defective", "wrong_item", "changed_mind", "damaged", "expired", "other"];
const REFUNDS: PaymentMethod[] = ["cash", "card_cib", "edahabia", "baridimob", "transfer"];
/** RET-007: back to the customer's account, when the ticket had one: less kridi, or store credit. */
const TO_ACCOUNT: PaymentMethod[] = ["credit", "voucher"];

interface Row {
  key: string;
  originLineId: string | null;
  productId: string | null;
  name: string;
  /** What is left to return on the original ticket; `null` without a receipt. */
  remaining: number | null;
  decimals: number;
  qty: string;
  price: string;
  reason: ReturnReason;
  restock: boolean;
  /** The ticket line's product: its serials sold may come back (M8, SAL-025). */
  soldProductId?: string | null;
}

type Step = "find" | "lines" | "confirm";

/** M8: the serials chosen per row, as the return input's lines (the rows with a quantity). */
function serialsOf(rows: Row[], chosen: Record<string, string[]>): ReturnSerial[] {
  return rows
    .filter((row) => row.qty.trim() !== "")
    .flatMap((row, line) => (chosen[row.key] ?? []).map((serialNo) => ({ line, serialNo })));
}

function ticketRow(r: ReturnableLine): Row {
  return {
    key: r.line.id,
    originLineId: r.line.id,
    productId: null,
    soldProductId: r.line.productId,
    name: r.line.description,
    remaining: r.remaining,
    decimals: r.line.unitCode === "pc" ? 0 : 3,
    qty: "",
    price: "",
    reason: "changed_mind",
    restock: true,
  };
}

/** F8 (spec §7.2, RET-001…011): the ticket, then the lines, then the exact refund (a dry run on
 * the backend), then a refund or an exchange. */
export function ReturnDialog({
  number,
  ctx,
  onClose,
}: {
  number?: string;
  ctx: PosContext;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const find = useFindDocument();
  const create = useCreateReturn();
  const hw = useHardwareConfig();
  const [step, setStep] = useState<Step>("find");
  const [ticket, setTicket] = useState(number ?? "");
  const [original, setOriginal] = useState<Document | null>(null);
  const [noReceipt, setNoReceipt] = useState(false);
  const [rows, setRows] = useState<Row[]>([]);
  const [refund, setRefund] = useState<PaymentMethod>("cash");
  const [reference, setReference] = useState("");
  const [search, setSearch] = useState("");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [error, setError] = useState<unknown>(null);
  const [quoted, setQuoted] = useState<{
    input: ReturnInput;
    quote: ReturnQuote;
    extras: ReturnExtras | null;
  } | null>(null);
  const [serials, setSerials] = useState<Record<string, string[]>>({});
  const returnable = useReturnable(original?.id ?? null);
  const accountId = noReceipt ? null : (original?.customerId ?? null);
  const customer = useCustomer(accountId);
  // M8: without a customer, a refund to a voucher is an exchange voucher (RET-007, gift vouchers)
  const modules = usePackModules();
  const exchangeVouchers = accountId === null && modules.includes("gift_vouchers");
  const refunds =
    accountId !== null
      ? [...REFUNDS, ...TO_ACCOUNT]
      : exchangeVouchers
        ? [...REFUNDS, "voucher" as const]
        : REFUNDS;
  const text = useDebounced(search.trim(), 150);
  const hits = useProductSearch(
    { text, limit: 6, categoryId: null, includeArchived: false },
    noReceipt && text.length >= 2,
  );

  const lookup = (value: string) => {
    const code = normalizeDigits(value.trim()).toUpperCase();
    if (code === "") return;
    find.mutate(code, {
      onSuccess: (doc) => {
        if (!doc) {
          setError({ code: "document_not_found" });
          return;
        }
        setError(null);
        setOriginal(doc);
        setStep("lines");
      },
      onError: setError,
    });
  };

  // a ticket sent by the sales history is looked up straight away
  const lookupSent = useEffectEvent(lookup);
  useEffect(() => {
    if (number) lookupSent(number);
  }, [number]);

  // the lines of the original ticket still to return, with what the cashier typed on them
  const fromTicket = (current: Row[]): Row[] =>
    (returnable.data ?? [])
      .filter((r) => r.remaining > 0)
      .map((r) => current.find((row) => row.originLineId === r.line.id) ?? ticketRow(r));
  const shown = noReceipt ? rows : fromTicket(rows);
  // M8: the serials coming back (a part of a line that sold serials) and the exchange voucher
  const returnSerials = serialsOf(shown, serials);
  const exchangeVoucher = exchangeVouchers && refund === "voucher";
  const packExtras: ReturnExtras | null =
    exchangeVoucher || returnSerials.length > 0 ? { exchangeVoucher, serials: returnSerials } : null;
  const quote = useReturnQuote(packExtras);
  const patch = (key: string, change: Partial<Row>) => {
    // M8: the serials ticked were for the old quantity; the picker asks them again (and none go
    // when the whole line comes back, which brings its serials back by itself, core's rule)
    if (change.qty !== undefined && change.qty !== shown.find((row) => row.key === key)?.qty) {
      setSerials((all) => ({ ...all, [key]: [] }));
    }
    setRows((current) =>
      (noReceipt ? current : fromTicket(current)).map((row) =>
        row.key === key ? { ...row, ...change } : row,
      ),
    );
  };

  const build = (): ReturnInput | null => {
    const found: Record<string, string> = {};
    const lines = shown.flatMap((row) => {
      if (row.qty.trim() === "") return [];
      const qty = parseQty(row.qty, row.decimals);
      if (qty === null) {
        found[row.key] = t("validation:qtyFormat");
        return [];
      }
      if (row.remaining !== null && qty > row.remaining) {
        found[row.key] = t("pos:returns.tooMany", { max: formatQty(row.remaining, lang) });
        return [];
      }
      const unitPrice = noReceipt ? parseAmount(row.price) : null;
      if (noReceipt && unitPrice === null) {
        found[`price-${row.key}`] = t("validation:amountFormat");
        return [];
      }
      return [
        {
          originLineId: row.originLineId,
          productId: row.productId,
          qty,
          unitPrice,
          restock: row.restock,
          reason: row.reason,
          note: null,
        },
      ];
    });
    if (lines.length === 0 && Object.keys(found).length === 0) found.none = t("pos:returns.nothing");
    setErrors(found);
    if (Object.keys(found).length > 0) return null;
    return {
      originalId: noReceipt ? null : (original?.id ?? null),
      lines,
      refundMethod: refund,
      refundReference: !needsReference(refund) || reference.trim() === "" ? null : reference.trim(),
      lang: original?.lang ?? lang,
      note: null,
    };
  };

  const toConfirm = async () => {
    const input = build();
    if (!input) return;
    setError(null);
    try {
      const result = await ctx.run(() => quote.mutateAsync(input));
      setQuoted({ input, quote: result, extras: packExtras });
      setStep("confirm");
    } catch (e) {
      if (!isApprovalCancelled(e)) setError(e);
    }
  };

  const refundNow = async () => {
    if (!quoted) return;
    // the shop's receipt setting, as at the payment dialog (SAL-052), when this PC has a printer
    const printReceipt = Boolean(hw.data?.receiptPrinterId) && (hw.data?.receipt?.printByDefault ?? true);
    try {
      const result = await ctx.run(() =>
        create.mutateAsync({ input: quoted.input, printReceipt, extras: quoted.extras ?? undefined }),
      );
      ctx.open({ kind: "done", done: { kind: "return", result } });
    } catch (e) {
      if (!isApprovalCancelled(e)) setError(e);
    }
  };

  const exchange = () => {
    if (!quoted) return;
    // an exchange spends the returned value on the new ticket; the return's extras go with it: the
    // serials coming back, and an exchange-voucher refund stays one (the rest becomes the voucher,
    // never M2's store credit, which needs a customer)
    useCart.getState().startExchange({
      input: quoted.input,
      value: quoted.quote.value,
      originalNumber: original?.number ?? null,
      ...(quoted.extras ? { extras: quoted.extras } : {}),
    });
    onClose();
  };

  const shownError = error ?? returnable.error;
  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("pos:returns.title")}
    >
      {step === "find" ? (
        <form
          noValidate
          onSubmit={(event: FormEvent) => {
            event.preventDefault();
            lookup(ticket);
          }}
          className="flex flex-col gap-3"
        >
          <Field id="return-ticket" label={t("pos:returns.ticket")}>
            <ScanInput
              id="return-ticket"
              dir="ltr"
              autoFocus
              value={ticket}
              onChange={(e) => setTicket(e.target.value)}
              onScan={(code) => {
                setTicket(code);
                lookup(code);
              }}
            />
          </Field>
          <div className="flex flex-wrap gap-2">
            <Button type="submit" variant="primary" disabled={find.isPending}>
              {t("pos:returns.find")}
            </Button>
            <Button
              variant="outline"
              onClick={() => {
                setError(null);
                setNoReceipt(true);
                setStep("lines");
              }}
            >
              {t("pos:returns.noReceipt")}
            </Button>
          </div>
        </form>
      ) : null}

      {step === "lines" ? (
        <div className="flex flex-col gap-4">
          {original ? (
            <div className="flex flex-col gap-1">
              <p className="font-semibold">
                {t("pos:returns.original")} <bdi dir="ltr">{original.number}</bdi>
              </p>
              {customer.data ? (
                <p>
                  {t("pos:customer.label")} {customer.data.name}
                </p>
              ) : null}
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <p className="rounded-control bg-warn-soft p-3 text-warn">{t("pos:returns.noReceiptHint")}</p>
              <Field id="return-search" label={t("pos:returns.search")}>
                <ScanInput
                  id="return-search"
                  dir="auto"
                  autoFocus
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  onScan={setSearch}
                />
              </Field>
              <div className="flex flex-wrap gap-2">
                {(hits.data ?? []).map((hit) => (
                  <Button
                    key={hit.id}
                    variant="outline"
                    onClick={() => {
                      setRows((all) => [
                        ...all,
                        {
                          key: `${hit.id}-${all.length}`,
                          originLineId: null,
                          productId: hit.id,
                          name: localizedName(hit, lang),
                          remaining: null,
                          decimals: hit.unitCode === "pc" ? 0 : 3,
                          qty: "",
                          price: formatAmountInput(hit.sellPrice, lang),
                          reason: "changed_mind",
                          restock: true,
                        },
                      ]);
                      setSearch("");
                    }}
                  >
                    {localizedName(hit, lang)} <Amount value={hit.sellPrice} />
                  </Button>
                ))}
              </div>
            </div>
          )}
          {original && returnable.isSuccess && shown.length === 0 ? (
            <p className="text-ink-2">{t("pos:returns.allReturned")}</p>
          ) : null}
          <ul className="flex flex-col gap-3">
            {shown.map((row) => (
              <li key={row.key} className="grid gap-2 rounded-card border border-line p-3 sm:grid-cols-4">
                <p className="font-semibold sm:col-span-4">
                  {row.name}
                  {row.remaining !== null ? (
                    <span className="ms-2 text-sm text-ink-2">
                      {t("pos:returns.left", { qty: formatQty(row.remaining, lang) })}
                    </span>
                  ) : null}
                </p>
                <Field
                  id={`qty-${row.key}`}
                  label={t("pos:returns.qty", { name: row.name })}
                  error={errors[row.key]}
                >
                  <NumericInput
                    id={`qty-${row.key}`}
                    kind="qty"
                    value={row.qty}
                    onValueChange={(qty) => patch(row.key, { qty })}
                  />
                </Field>
                {noReceipt ? (
                  <Field
                    id={`price-${row.key}`}
                    label={t("pos:returns.price")}
                    error={errors[`price-${row.key}`]}
                  >
                    <NumericInput
                      id={`price-${row.key}`}
                      value={row.price}
                      onValueChange={(price) => patch(row.key, { price })}
                    />
                  </Field>
                ) : null}
                <Field id={`reason-${row.key}`} label={t("pos:returns.reason", { name: row.name })}>
                  <NativeSelect
                    id={`reason-${row.key}`}
                    value={row.reason}
                    onChange={(e) => patch(row.key, { reason: e.target.value as ReturnReason })}
                  >
                    {REASONS.map((r) => (
                      <option key={r} value={r}>
                        {t(`pos:returns.reasons.${r}`)}
                      </option>
                    ))}
                  </NativeSelect>
                </Field>
                {/* unticked, the goods go to the damaged-goods location (RET-006) */}
                <label className="flex items-center gap-2 self-end pb-3 font-semibold">
                  <Checkbox
                    checked={row.restock}
                    onCheckedChange={(v) => patch(row.key, { restock: v === true })}
                  />
                  {t("pos:returns.restock")}
                </label>
                {/* M8 (SAL-025): which of the line's serials come back, when not all of them */}
                {modules.includes("serials") && row.originLineId && row.soldProductId ? (
                  <Suspense fallback={null}>
                    <ReturnSerials
                      productId={row.soldProductId}
                      lineId={row.originLineId}
                      name={row.name}
                      qty={parseQty(row.qty, row.decimals) ?? 0}
                      value={serials[row.key] ?? []}
                      onChange={(chosen) => setSerials((all) => ({ ...all, [row.key]: chosen }))}
                    />
                  </Suspense>
                ) : null}
              </li>
            ))}
          </ul>
          <RadioCards
            legend={t("pos:returns.refundMethod")}
            name="refund-method"
            value={refund}
            onChange={setRefund}
            options={refunds.map((m) => ({
              value: m,
              label:
                m === "voucher" && accountId === null
                  ? t("pos:returns.exchangeVoucher")
                  : t(`payments:tenders.${m}`),
            }))}
          />
          {needsReference(refund) ? (
            <Field id="refund-ref" label={t("pos:returns.reference")}>
              <Input
                id="refund-ref"
                dir="ltr"
                value={reference}
                maxLength={40}
                onChange={(e) => setReference(e.target.value)}
              />
            </Field>
          ) : null}
          {errors.none ? (
            <p role="alert" className="text-danger">
              {errors.none}
            </p>
          ) : null}
          <div>
            <Button variant="primary" onClick={() => void toConfirm()} disabled={quote.isPending}>
              {t("pos:returns.next")}
            </Button>
          </div>
        </div>
      ) : null}

      {step === "confirm" && quoted ? (
        <div className="flex flex-col gap-4">
          <div
            role="status"
            aria-label={t("pos:returns.toRefund")}
            className="flex items-baseline justify-between rounded-control bg-info-soft p-4 text-info"
          >
            <span className="text-lg font-semibold">{t("pos:returns.toRefund")}</span>
            <Amount value={quoted.quote.value} className="text-3xl font-bold" />
          </div>
          <div className="flex flex-wrap gap-2">
            <Button variant="primary" onClick={() => void refundNow()} disabled={create.isPending}>
              {t("pos:returns.refund")}
            </Button>
            {ctx.session.permissions.includes("sales.create") ? (
              <Button variant="outline" onClick={exchange}>
                {t("pos:returns.exchange")}
              </Button>
            ) : null}
            <Button variant="ghost" onClick={() => setStep("lines")}>
              {t("pos:returns.change")}
            </Button>
          </div>
        </div>
      ) : null}

      {shownError ? (
        <p role="alert" className="text-danger">
          {errorText(shownError)}
        </p>
      ) : null}
    </Dialog>
  );
}
