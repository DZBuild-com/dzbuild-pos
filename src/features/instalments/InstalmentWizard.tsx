import { zodResolver } from "@hookform/resolvers/zod";
import { useQueryClient } from "@tanstack/react-query";
import { CircleCheck, Trash2 } from "lucide-react";
import { type KeyboardEvent, type Ref, useImperativeHandle, useMemo, useState } from "react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { z } from "zod";
import { Amount } from "@/components/amount";
import { useApproval } from "@/components/manager-approval";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { CustomerField } from "@/features/documents/editor/CustomerField";
import { ProductPicker } from "@/features/purchases/purchase-lines";
import { isApprovalCancelled } from "@/lib/approval";
import { dayToMs, isDay, today } from "@/lib/dates";
import { formatAmountInput, formatDate, formatDZD } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import { usePriceLevels } from "@/lib/ipc/catalog";
import { toAppError } from "@/lib/ipc/client";
import { useCustomer } from "@/lib/ipc/customers";
import { useHardwareConfig } from "@/lib/ipc/hardware";
import { useInstalmentQuote, useSignInstalment } from "@/lib/ipc/instalments";
import { useProduct } from "@/lib/ipc/products";
import { useSetting } from "@/lib/ipc/settings";
import { type PackAnswers, PackSteps } from "@/features/pos/pack-steps";
import { commands } from "@/lib/ipc/client";
import { qk } from "@/lib/ipc/keys";
import { usePackModules } from "@/lib/ipc/pack-state";
import type {
  BuyerIdentity,
  Cart,
  CartLine,
  PaymentMethod,
  PrescriptionInput,
  QuoteInput,
  SaleExtras,
  SignedDone,
  SignInput,
} from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty, parseAmount, parseQty } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { ContractPrintDialog } from "./ContractPrintDialog";
import { EMPTY_GUARANTEE, GuaranteeFields, GuaranteeList, isFilled, toGuarantee } from "./Guarantees";
import type { GuaranteeDraft } from "./Guarantees";
import { PAY_METHODS } from "./kinds";

const STEPS = ["customer", "goods", "terms", "documents", "sign"] as const;
/** `instalments.max_count`'s default, while the setting is being read. */
const DEFAULT_MAX = 24;
/** The code of the instalment price level the backend seeds (TMP-002). */
const LEVEL_CODE = "instalment";

/** One article of the contract: the till's line as it was (its discount kept), or one added here. */
interface Good {
  key: number;
  line: CartLine;
  /** The name picked here; the till's lines read theirs from the catalogue. */
  name: string | null;
  qty: string;
  /** SAL-025: its serials (IMEIs), given at the till or at "Signer" (final review). */
  serials: string[];
  /** Its controlled-medicine register entry (`line` re-numbered when the contract is signed). */
  prescription: PrescriptionInput | null;
}

let nextKey = 1;

/** The till's cart, or one product chosen on `/instalments`. */
export type WizardSeed = Cart | { productId: string; qty: number };

/** What a dialog around the wizard asks it (Radix reads Esc before the wizard's own handler). */
export interface WizardHandle {
  /** Esc: back one step and `true` from the goods, terms and documents steps; `false` on the
   * first and the last step, where Esc closes the dialog. */
  escape: () => boolean;
}

function plainLine(productId: string, qty: number): CartLine {
  return {
    productId,
    description: null,
    qty,
    unitPrice: null,
    discount: { kind: "none" },
    taxRateId: null,
    note: null,
    expectedUnitPrice: null,
  };
}

/** The goods of `seed`, each with what the till's extras gave its line (by the line's index). */
function goodsOf(seed: WizardSeed | undefined, lang: Lang, extras: SignInput["extras"]): Good[] {
  if (!seed) return [];
  const lines = "lines" in seed ? seed.lines : [plainLine(seed.productId, seed.qty)];
  return lines.map((line, index) => ({
    key: nextKey++,
    line,
    name: null,
    qty: formatQty(line.qty, lang),
    serials: (extras?.serials ?? []).filter((s) => s.line === index).map((s) => s.serialNo),
    prescription: extras?.prescriptions?.find((p) => p.line === index) ?? null,
  }));
}

/** The contract's pack part from the goods as they are now (their order is the ticket's lines):
 * the serials and register entries re-numbered by line, the buyer, the till's van. */
function extrasOf(
  goods: readonly Good[],
  buyer: BuyerIdentity | null,
  from: SignInput["extras"],
): SaleExtras {
  return {
    serials: goods.flatMap((g, line) => g.serials.map((serialNo) => ({ line, serialNo }))),
    prescriptions: goods.flatMap((g, line) => (g.prescription ? [{ ...g.prescription, line }] : [])),
    buyer: buyer ?? undefined,
    paymentTypes: [],
    locationId: from?.locationId ?? undefined,
  };
}

/** The goods with what the pack steps answered for each (by its line). */
function answered(goods: readonly Good[], answers: PackAnswers): Good[] {
  return goods.map((g, line) => ({
    ...g,
    serials: answers.serials[line] ?? g.serials,
    prescription: answers.prescriptions.find((p) => p.line === line) ?? g.prescription,
  }));
}

/** The same day of the next month (the last day of a shorter month): the first due's default. */
function nextMonth(day: string): string {
  const [y = 0, m = 1, d = 1] = day.split("-").map(Number);
  const last = new Date(Date.UTC(y, m + 1, 0)).getUTCDate();
  return new Date(Date.UTC(y, m, Math.min(d, last))).toISOString().slice(0, 10);
}

const termsSchema = (max: number, signatureDay: string) =>
  z.object({
    down: z.string().refine((v) => {
      const amount = parseAmount(v);
      return amount !== null && amount >= 0;
    }, "validation:amountFormat"),
    downMethod: z.string(),
    count: z.string().refine((v) => {
      const n = Number(v);
      return Number.isInteger(n) && n >= 1 && n <= max;
    }, "errors:instalment_count_invalid"),
    firstDueOn: z.string().refine((v) => isDay(v) && v > signatureDay, "instalments:wizard.firstDueAfter"),
    reference: z.string().max(40, "validation:tooLong"),
    chequeBank: z.string().max(48, "validation:tooLong"),
    chequeDate: z.string(),
  });
type Terms = z.infer<ReturnType<typeof termsSchema>>;

const text = (value: string) => (value.trim() === "" ? null : value.trim());
const methodOf = (value: string): PaymentMethod => PAY_METHODS.find((m) => m === value) ?? "cash";

/**
 * TMP-001/002/009: an instalment sale in five steps — the customer (his kridi balance and limit),
 * the goods at the instalment price level, the down payment and the schedule (every figure from
 * `instalments_quote`: the total, the droit de timbre on the down payment's cash, what to receive
 * now, what is financed and the dues), the supporting documents (a closed list: never a cheque),
 * then "Signer et encaisser l'apport" (`instalments_sign`: the contract's ticket prints) and the
 * contract to print in the customer's language. Esc goes back one step.
 */
export function InstalmentWizard({
  ref,
  initialCart,
  extras,
  onSigned,
  onDone,
}: {
  /** For a dialog around the wizard: its Esc goes back a step through `escape()`. */
  ref?: Ref<WizardHandle>;
  initialCart?: WizardSeed;
  /** The till's pack extras for the contract's ticket (serials, buyer…), when started from it. */
  extras?: SignInput["extras"];
  /** Right after the signature: the till empties its cart (the ticket is sold). */
  onSigned?: (done: SignedDone) => void;
  /** "Terminer" once signed. */
  onDone?: (planId: string) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [step, setStep] = useState(0);
  const [customerId, setCustomerId] = useState<string | null>(
    initialCart && "lines" in initialCart ? initialCart.customerId : null,
  );
  const [goods, setGoods] = useState<Good[]>(() => goodsOf(initialCart, lang, extras));
  // the jewel's buyer (the till's, or asked at "Signer"), and the pack steps asked then
  const [buyer, setBuyer] = useState<BuyerIdentity | null>(extras?.buyer ?? null);
  const [asking, setAsking] = useState<{
    lines: { productId: string | null; name: string; qty: number }[];
  } | null>(null);
  const modules = usePackModules();
  const client = useQueryClient();
  const [guarantees, setGuarantees] = useState<GuaranteeDraft[]>([]);
  const [draft, setDraft] = useState<GuaranteeDraft>(EMPTY_GUARANTEE);
  const [signed, setSigned] = useState<SignedDone | null>(null);
  const [printing, setPrinting] = useState(false);
  const [signError, setSignError] = useState<unknown>(null);
  const customer = useCustomer(customerId);
  const levels = usePriceLevels();
  const maxSetting = useSetting("instalments.max_count");
  const max = typeof maxSetting.data === "number" ? maxSetting.data : DEFAULT_MAX;
  const signatureDay = today();
  const hardware = useHardwareConfig();
  const noPrinter = hardware.data !== undefined && !hardware.data.receiptPrinterId;
  const sign = useSignInstalment();
  const approval = useApproval();

  const schema = useMemo(() => termsSchema(max, signatureDay), [max, signatureDay]);
  const { control, register, formState } = useForm<Terms>({
    resolver: zodResolver(schema),
    mode: "onChange",
    defaultValues: {
      down: formatAmountInput(0, lang),
      downMethod: "cash",
      count: String(Math.min(12, max)),
      firstDueOn: nextMonth(signatureDay),
      reference: "",
      chequeBank: "",
      chequeDate: "",
    },
  });
  const values = useWatch({ control }) as Terms;
  const method = methodOf(values.downMethod);

  // The cart as the backend prices it: the instalment level (TMP-002) and the contract's customer.
  const level = levels.data?.find((l) => l.code === LEVEL_CODE && !l.archived)?.id ?? null;
  const lines = goods.flatMap((g) => {
    const qty = parseQty(g.qty, 3);
    return qty === null ? [] : [{ ...g.line, qty }];
  });
  const goodsReady = goods.length > 0 && lines.length === goods.length;
  const cart: Cart = {
    lines,
    priceLevelId: level,
    customerId,
    lang: customer.data?.lang ?? lang,
    note: initialCart && "lines" in initialCart ? initialCart.note : null,
  };
  const terms = schema.safeParse(values);
  const input: QuoteInput | null =
    terms.success && customerId !== null && goodsReady
      ? {
          cart,
          down: parseAmount(values.down) ?? 0,
          downMethod: method,
          count: Number(values.count),
          firstDueOn: values.firstDueOn,
        }
      : null;
  // The quote follows the form once it rests (a query per settled change, not per key).
  const key = input ? JSON.stringify(input) : null;
  const settled = useDebounced(key, 300);
  const quoteInput = useMemo(() => (settled ? (JSON.parse(settled) as QuoteInput) : null), [settled]);
  const quote = useInstalmentQuote(step >= 2 && !signed ? quoteInput : null);
  // Only figures of the terms on screen: never a previous answer while the next one comes.
  const fresh =
    key !== null && key === settled && quote.isSuccess && !quote.isPlaceholderData && !quote.isFetching;
  const figures = fresh ? quote.data : undefined;
  const quoteError = key !== null && key === settled && !quote.isFetching ? quote.error : null;
  const errorField = quoteError ? toAppError(quoteError).field : null;
  const fieldError = (field: keyof Terms) => {
    const message = formState.errors[field]?.message;
    if (message) return t(message, { max });
    return errorField === field ? errorText(quoteError) : undefined;
  };

  const canNext = [customerId !== null, goodsReady, figures !== undefined, true, false][step] ?? false;
  const back = () => setStep((s) => Math.max(0, s - 1));
  const escape = () => {
    if (step <= 0 || step >= 4) return false;
    back();
    return true;
  };
  useImperativeHandle(ref, () => ({ escape }));
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    // In a dialog, its Esc (read first, on the document) has already gone back: not twice
    if (event.key === "Escape" && !event.defaultPrevented && escape()) event.preventDefault();
  };

  /** The goods' names for the pack steps' dialogs (the catalogue's, read by their rows). */
  const nameOf = async (good: Good): Promise<string> => {
    if (good.name) return good.name;
    if (good.line.description) return good.line.description;
    const id = good.line.productId;
    if (!id) return "";
    const product = await client
      .fetchQuery({ queryKey: qk.product(id), queryFn: () => commands.productsGet(id) })
      .catch(() => null);
    return product ? localizedName(product, lang) : "";
  };

  /** "Signer": the serials, register entries and buyer the contract's ticket needs first (the
   * till's "Encaisser" steps, final review), then the signature. */
  const startSigning = async () => {
    if (!quoteInput || !figures) return;
    const packs = modules.some((m) => m === "serials" || m === "pharmacy" || m === "jewellery");
    if (!packs) return submit(goods, buyer);
    const names = await Promise.all(goods.map(nameOf));
    setAsking({
      lines: goods.map((g, index) => ({
        productId: g.line.productId,
        name: names[index] ?? "",
        qty: parseQty(g.qty, 3) ?? 0,
      })),
    });
  };

  const submit = async (signed: readonly Good[], identity: BuyerIdentity | null) => {
    if (!quoteInput || !figures) return;
    setSignError(null);
    const documents = [...guarantees, ...(isFilled(draft) ? [draft] : [])].map(toGuarantee);
    const signInput: SignInput = {
      quote: quoteInput,
      downReference: method === "cash" ? null : text(values.reference),
      chequeBank: method === "cheque" ? text(values.chequeBank) : null,
      chequeDate: method === "cheque" ? text(values.chequeDate) : null,
      guarantees: documents,
      extras: extrasOf(signed, identity, extras),
      note: null,
    };
    try {
      const done = await approval.run(() => sign.mutateAsync({ input: signInput, printReceipt: !noPrinter }));
      setSigned(done);
      setStep(4);
      onSigned?.(done);
    } catch (error) {
      if (!isApprovalCancelled(error)) setSignError(error);
    }
  };

  return (
    // Esc inside the wizard goes back one step (the keys of its fields bubble here)
    <div className="flex flex-col gap-6" onKeyDown={onKeyDown}>
      <Progress step={step} />
      {step === 0 ? (
        <section className="flex flex-col gap-4">
          <h2 className="text-xl font-bold">{t("instalments:wizard.steps.customer")}</h2>
          <CustomerField value={customerId} onChange={setCustomerId} />
          {customer.data ? (
            <dl className="grid max-w-xl grid-cols-[auto_1fr] gap-x-6 gap-y-1 rounded-card bg-surface-2 p-4">
              <dt className="text-ink-2">{t("instalments:wizard.balance")}</dt>
              <dd className="text-end">
                <Amount value={customer.data.balance} />
              </dd>
              <dt className="text-ink-2">{t("instalments:wizard.creditLimit")}</dt>
              <dd className="text-end">
                {customer.data.creditLimit === null ? (
                  t("instalments:wizard.noLimit")
                ) : (
                  <Amount value={customer.data.creditLimit} />
                )}
              </dd>
              {customer.data.availableCredit !== null ? (
                <>
                  <dt className="text-ink-2">{t("instalments:wizard.available")}</dt>
                  <dd className="text-end font-semibold">
                    <Amount value={customer.data.availableCredit} />
                  </dd>
                </>
              ) : null}
            </dl>
          ) : null}
          {customer.data?.isBlocked ? (
            <p role="alert" className="text-danger">
              {t("instalments:wizard.blocked")}
            </p>
          ) : null}
        </section>
      ) : null}

      {step === 1 ? (
        <section className="flex flex-col gap-4">
          <h2 className="text-xl font-bold">{t("instalments:wizard.steps.goods")}</h2>
          <p className="text-ink-2">{t("instalments:wizard.goodsHint")}</p>
          {goods.length === 0 ? <p className="text-ink-2">{t("instalments:wizard.noGoods")}</p> : null}
          <ul className="flex flex-col gap-2">
            {goods.map((good) => (
              <GoodRow
                key={good.key}
                good={good}
                // a new quantity asks its serials again (one per piece)
                onQty={(qty) =>
                  setGoods((all) => all.map((g) => (g.key === good.key ? { ...g, qty, serials: [] } : g)))
                }
                onRemove={() => setGoods((all) => all.filter((g) => g.key !== good.key))}
              />
            ))}
          </ul>
          <ProductPicker
            onPick={(p) =>
              setGoods((all) => [
                ...all,
                {
                  key: nextKey++,
                  line: plainLine(p.productId, p.packQty ?? 1_000),
                  name: p.name,
                  qty: formatQty(p.packQty ?? 1_000, lang),
                  serials: [],
                  prescription: null,
                },
              ])
            }
          />
        </section>
      ) : null}

      {step === 2 ? (
        <section className="flex flex-col gap-4">
          <h2 className="text-xl font-bold">{t("instalments:wizard.steps.terms")}</h2>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field id="instalment-down" label={t("instalments:wizard.down")} error={fieldError("down")}>
              <Controller
                control={control}
                name="down"
                render={({ field }) => (
                  <NumericInput
                    id="instalment-down"
                    value={field.value}
                    onValueChange={field.onChange}
                    onBlur={field.onBlur}
                    className="text-xl font-bold"
                  />
                )}
              />
            </Field>
            <Field
              id="instalment-method"
              label={t("instalments:wizard.downMethod")}
              error={fieldError("downMethod")}
            >
              <NativeSelect id="instalment-method" {...register("downMethod")}>
                {PAY_METHODS.map((m) => (
                  <option key={m} value={m}>
                    {t(`payments:tenders.${m}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            {method !== "cash" ? (
              <Field
                id="instalment-reference"
                label={t(`payments:reference.${method}`, { method: t(`payments:tenders.${method}`) })}
                error={fieldError("reference")}
              >
                <Controller
                  control={control}
                  name="reference"
                  render={({ field }) => (
                    <GuardedInput
                      id="instalment-reference"
                      dir="ltr"
                      maxLength={40}
                      value={field.value}
                      onValueChange={field.onChange}
                    />
                  )}
                />
              </Field>
            ) : null}
            {method === "cheque" ? (
              <>
                <Field
                  id="instalment-bank"
                  label={t("payments:cheque.bank")}
                  error={fieldError("chequeBank")}
                >
                  <Input id="instalment-bank" maxLength={48} {...register("chequeBank")} />
                </Field>
                <Field id="instalment-cheque-date" label={t("payments:cheque.date")}>
                  <Input id="instalment-cheque-date" type="date" dir="ltr" {...register("chequeDate")} />
                </Field>
              </>
            ) : null}
            <Field id="instalment-count" label={t("instalments:wizard.count")} error={fieldError("count")}>
              <NativeSelect id="instalment-count" {...register("count")}>
                {Array.from({ length: max }, (_, i) => String(i + 1)).map((n) => (
                  <option key={n} value={n}>
                    {n}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            <Field
              id="instalment-first-due"
              label={t("instalments:wizard.firstDue")}
              error={fieldError("firstDueOn")}
            >
              <Input id="instalment-first-due" type="date" dir="ltr" {...register("firstDueOn")} />
            </Field>
          </div>
          <QuoteFigures figures={figures} pending={key !== null && !fresh && !quoteError} />
          {quoteError && !errorField ? (
            <p role="alert" className="text-danger">
              {errorText(quoteError)}
            </p>
          ) : null}
        </section>
      ) : null}

      {step === 3 ? (
        <section className="flex flex-col gap-4">
          <h2 className="text-xl font-bold">{t("instalments:wizard.steps.documents")}</h2>
          <p role="note" className="rounded-control bg-info-soft p-3 text-info">
            {t("instalments:guarantees.noCheque")}
          </p>
          <GuaranteeList
            items={guarantees.map(toGuarantee)}
            onRemove={(index) => setGuarantees((all) => all.filter((_, i) => i !== index))}
          />
          <GuaranteeFields id="instalment-guarantee" value={draft} onChange={setDraft} />
          <Button
            variant="secondary"
            className="self-start"
            disabled={!isFilled(draft)}
            onClick={() => {
              setGuarantees((all) => [...all, draft]);
              setDraft(EMPTY_GUARANTEE);
            }}
          >
            {t("instalments:guarantees.add")}
          </Button>
          {figures ? (
            <p className="text-xl font-bold">
              {t("instalments:wizard.toReceive", { amount: formatDZD(figures.downToReceive, lang) })}
            </p>
          ) : null}
          {signError ? (
            <p role="alert" className="text-danger">
              {errorText(signError)}
            </p>
          ) : null}
        </section>
      ) : null}

      {step === 4 && signed ? (
        <section className="flex flex-col items-start gap-4">
          <p role="status" className="flex items-center gap-2 text-2xl font-bold text-ok">
            <CircleCheck aria-hidden className="size-7" />
            {t("instalments:wizard.done", { number: signed.signed.plan.number })}
          </p>
          <p>
            {t("instalments:wizard.ticket", { number: signed.signed.sale.document.number ?? "" })}
            {signed.printQueued ? ` ${t("instalments:wizard.printing")}` : ""}
          </p>
        </section>
      ) : null}

      <div className="flex flex-wrap items-center justify-end gap-2 border-t border-line pt-4">
        {step > 0 && step < 4 ? (
          <Button variant="outline" onClick={back}>
            {t("instalments:wizard.previous")}
          </Button>
        ) : null}
        {step < 3 ? (
          <Button variant="primary" disabled={!canNext} onClick={() => setStep((s) => s + 1)}>
            {t("common:actions.next")}
          </Button>
        ) : null}
        {step === 3 ? (
          <Button
            variant="primary"
            disabled={!figures || sign.isPending || asking !== null}
            onClick={() => void startSigning()}
          >
            {t("instalments:wizard.sign")}
          </Button>
        ) : null}
        {step === 4 && signed ? (
          <>
            <Button variant="primary" onClick={() => setPrinting(true)}>
              {t("instalments:wizard.printContract")}
            </Button>
            <Button variant="outline" onClick={() => onDone?.(signed.signed.plan.id)}>
              {t("instalments:wizard.finish")}
            </Button>
          </>
        ) : null}
      </div>
      {asking ? (
        <PackSteps
          lines={asking.lines}
          customerId={customerId}
          modules={modules}
          answers={{
            serials: Object.fromEntries(goods.map((g, line) => [line, g.serials])),
            prescriptions: goods.flatMap((g, line) => (g.prescription ? [{ ...g.prescription, line }] : [])),
            buyer,
          }}
          onDone={(answers) => {
            const filled = answered(goods, answers);
            setGoods(filled);
            setBuyer(answers.buyer);
            setAsking(null);
            void submit(filled, answers.buyer);
          }}
          onClose={() => setAsking(null)}
        />
      ) : null}
      {printing && signed ? (
        <ContractPrintDialog
          planId={signed.signed.plan.id}
          lang={customer.data?.lang ?? lang}
          onClose={() => setPrinting(false)}
        />
      ) : null}
      {approval.dialog}
    </div>
  );
}

/** The five steps, the current one marked (M0's wizard frame: "Étape 2 sur 5"). */
function Progress({ step }: { step: number }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-2">
      <p className="text-sm text-ink-2">
        {t("instalments:wizard.progress", { step: step + 1, count: STEPS.length })}
      </p>
      <ol className="grid grid-cols-5 gap-2">
        {STEPS.map((name, index) => (
          <li
            key={name}
            aria-current={index === step ? "step" : undefined}
            className={cn(
              "flex flex-col gap-1 text-sm font-semibold",
              index === step ? "text-ink" : "text-ink-2",
            )}
          >
            <span
              aria-hidden
              className={cn("h-1.5 rounded-full", index <= step ? "bg-brand" : "bg-surface-2")}
            />
            {t(`instalments:wizard.steps.${name}`)}
          </li>
        ))}
      </ol>
    </div>
  );
}

/** An article: its name (the catalogue's in the UI language), its quantity, and a way to drop it. */
function GoodRow({
  good,
  onQty,
  onRemove,
}: {
  good: Good;
  onQty: (qty: string) => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const product = useProduct(good.name === null ? good.line.productId : null);
  const name =
    good.name ??
    good.line.description ??
    (product.data ? localizedName(product.data, lang) : t("common:status.loading"));
  const invalid = parseQty(good.qty, 3) === null;
  return (
    <li className="flex flex-wrap items-end gap-3 rounded-control border border-line p-3">
      <span className="min-w-48 flex-1 font-semibold">{name}</span>
      <div className="w-32">
        <Field
          id={`good-qty-${good.key}`}
          label={t("instalments:wizard.qty")}
          error={invalid ? t("validation:qtyFormat") : undefined}
        >
          <NumericInput id={`good-qty-${good.key}`} kind="qty" value={good.qty} onValueChange={onQty} />
        </Field>
      </div>
      <Button
        variant="ghost"
        size="icon"
        aria-label={t("instalments:wizard.remove", { name })}
        title={t("instalments:wizard.remove", { name })}
        onClick={onRemove}
      >
        <Trash2 aria-hidden />
      </Button>
    </li>
  );
}

/** The contract's figures and schedule, all from `instalments_quote`. */
function QuoteFigures({
  figures,
  pending,
}: {
  figures: ReturnType<typeof useInstalmentQuote>["data"];
  pending: boolean;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  if (!figures) {
    return pending ? (
      <p role="status" className="text-ink-2">
        {t("instalments:wizard.computing")}
      </p>
    ) : null;
  }
  return (
    <div aria-live="polite" className="flex flex-col gap-4">
      <div className="flex flex-col gap-1 rounded-card bg-surface-2 p-4">
        <p>{t("instalments:wizard.total", { amount: formatDZD(figures.total, lang) })}</p>
        {figures.stampDuty > 0 ? (
          <p>{t("instalments:wizard.duty", { amount: formatDZD(figures.stampDuty, lang) })}</p>
        ) : null}
        <p className="text-xl font-bold">
          {t("instalments:wizard.toReceive", { amount: formatDZD(figures.downToReceive, lang) })}
        </p>
        <p className="font-semibold">
          {t("instalments:wizard.financed", { amount: formatDZD(figures.financed, lang) })}
        </p>
      </div>
      <table aria-label={t("instalments:wizard.schedule")} className="w-full max-w-xl border-collapse">
        <thead>
          <tr className="border-b border-line text-ink-2">
            <th scope="col" className="py-2 text-start font-semibold">
              {t("instalments:plan.seq")}
            </th>
            <th scope="col" className="py-2 text-start font-semibold">
              {t("instalments:plan.dueOn")}
            </th>
            <th scope="col" className="py-2 text-end font-semibold">
              {t("instalments:plan.amount")}
            </th>
          </tr>
        </thead>
        <tbody>
          {figures.dues.map((due) => (
            <tr key={due.seq} className="border-b border-line even:bg-surface-2">
              <td className="py-2 tabular-nums">{due.seq}</td>
              <td className="py-2">
                <bdi dir="ltr">{formatDate(dayToMs(due.dueOn), lang)}</bdi>
              </td>
              <td className="py-2 text-end">
                <Amount value={due.amount} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <Badge tone="info" className="self-start">
        {t("instalments:wizard.monthly")}
      </Badge>
    </div>
  );
}
