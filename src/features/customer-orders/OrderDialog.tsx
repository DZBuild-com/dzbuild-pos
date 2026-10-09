import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useApproval } from "@/components/manager-approval";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { isApprovalCancelled } from "@/lib/approval";
import { today } from "@/lib/dates";
import { formatDZD } from "@/lib/format";
import { useCreateOrder } from "@/lib/ipc/orders";
import { useDepositQuote } from "@/lib/ipc/services";
import { useSetting } from "@/lib/ipc/settings";
import type { Cart, DepositInput, Order, OrderDeposit, OrderKind, PaymentMethod } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** How a deposit is paid (Task 9's tender rules: never credit nor a voucher). */
const METHODS: readonly PaymentMethod[] = ["cash", "card_cib", "edahabia", "baridimob", "transfer"];

/**
 * ORD-003: the least deposit of a layaway, `orders.layaway_min_deposit_bp` of its total, rounded
 * to the centime as the backend's `Money::apply_rate` rounds it (half away from zero), so the
 * screen never asks more than the server will accept. Shown before sending; the server checks
 * again (`layaway_deposit_too_low`).
 */
export function layawayMinimum(total: number, bp: number): number {
  return Number((BigInt(total) * BigInt(bp) * 2n + 10_000n) / 20_000n);
}

/**
 * ORD-002/003 "Mettre en commande": the till's cart (or a quote converted into an order draft)
 * becomes a customer order `CC…` or a layaway, due on a day, its stock reserved (always for a
 * layaway), with a deposit whose droit de timbre comes from `deposits_quote`; the layaway's
 * minimum deposit is shown and enforced before sending. "Enregistrer la commande" records it (the
 * deposit's `AC…` receipt prints) and the till empties its cart (`onCreated`).
 */
export function OrderDialog({
  cart,
  total,
  draftId = null,
  onCreated,
  onClose,
}: {
  cart: Cart;
  /** The cart's total TTC as the backend priced it (the till's preview, the draft's total). */
  total: number | null;
  /** The order draft converted from a quote (`documents_convert`); none at the till. */
  draftId?: string | null;
  onCreated?: (order: Order) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [kind, setKind] = useState<OrderKind>("order");
  const [due, setDue] = useState("");
  const [reserve, setReserve] = useState(false);
  const [amount, setAmount] = useState("");
  const [method, setMethod] = useState<PaymentMethod>("cash");
  const [reference, setReference] = useState("");
  const [note, setNote] = useState("");
  const [problem, setProblem] = useState<unknown>(null);
  const create = useCreateOrder();
  const approval = useApproval();
  const minSetting = useSetting("orders.layaway_min_deposit_bp");
  const bp = typeof minSetting.data === "number" ? minSetting.data : null;
  const layaway = kind === "layaway";
  const minimum = layaway && total !== null && bp !== null ? layawayMinimum(total, bp) : null;

  const typed = amount.trim() === "" ? 0 : parseAmount(amount);
  const deposit: OrderDeposit | null =
    typed !== null && typed > 0
      ? {
          amount: typed,
          method,
          reference: method === "cash" || reference.trim() === "" ? null : reference.trim(),
          chequeBank: null,
          chequeDate: null,
        }
      : null;
  const quoteInput: DepositInput | null = deposit && {
    ownerType: "customer_order",
    ownerId: draftId ?? "",
    amount: deposit.amount,
    method: deposit.method,
    reference: deposit.reference,
    chequeBank: null,
    chequeDate: null,
    note: null,
  };
  const quote = useDepositQuote(quoteInput);
  const belowMinimum = layaway && (minimum === null || (deposit?.amount ?? 0) < minimum);
  const noCustomer = cart.customerId === null;
  const dueInvalid = due !== "" && due < today();
  const ready =
    !noCustomer &&
    typed !== null &&
    !belowMinimum &&
    !dueInvalid &&
    cart.lines.length > 0 &&
    !create.isPending;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!ready) return;
    setProblem(null);
    try {
      const order = await approval.run(() =>
        create.mutateAsync({
          draftId,
          cart,
          kind,
          dueOn: due === "" ? null : due,
          reserve: layaway || reserve,
          deposit,
          note: note.trim() === "" ? null : note.trim(),
        }),
      );
      toast.success(t("orders:dialog.created", { number: order.number }));
      onCreated?.(order);
      onClose();
    } catch (error) {
      if (!isApprovalCancelled(error)) setProblem(error);
    }
  };

  return (
    <>
      <Dialog
        open={!approval.pending}
        size="lg"
        onOpenChange={(open) => (open ? undefined : onClose())}
        title={t("orders:dialog.title")}
        footer={
          <>
            <Button variant="outline" onClick={onClose}>
              {t("common:actions.cancel")}
            </Button>
            <Button type="submit" form="order-create" variant="primary" disabled={!ready}>
              {t("orders:dialog.submit")}
            </Button>
          </>
        }
      >
        <form id="order-create" noValidate onSubmit={(e) => void submit(e)} className="flex flex-col gap-4">
          {noCustomer ? (
            <p role="alert" className="text-danger">
              {t("errors:customer_required")}
            </p>
          ) : null}
          <RadioCards
            legend={t("orders:dialog.kind")}
            name="order-kind"
            value={kind}
            onChange={setKind}
            options={[
              { value: "order", label: t("orders:kinds.order") },
              { value: "layaway", label: t("orders:kinds.layaway") },
            ]}
          />
          <p className="text-ink-2">{t(layaway ? "orders:dialog.layawayHint" : "orders:dialog.orderHint")}</p>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field
              id="order-due"
              label={t("orders:dialog.due")}
              optional={t("orders:optional")}
              error={dueInvalid ? t("orders:dialog.dueInvalid") : undefined}
            >
              <Input
                id="order-due"
                type="date"
                dir="ltr"
                min={today()}
                value={due}
                onChange={(e) => setDue(e.target.value)}
              />
            </Field>
            <label className="flex min-h-11 items-center gap-3 self-end font-semibold">
              <Checkbox
                checked={layaway || reserve}
                disabled={layaway}
                onCheckedChange={(v) => setReserve(v === true)}
              />
              {t("orders:dialog.reserve")}
            </label>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field
              id="order-deposit"
              label={t("orders:dialog.deposit")}
              optional={layaway ? undefined : t("orders:optional")}
              error={typed === null ? t("validation:amountFormat") : undefined}
            >
              <NumericInput id="order-deposit" value={amount} onValueChange={setAmount} />
            </Field>
            <Field id="order-method" label={t("orders:dialog.method")}>
              <NativeSelect
                id="order-method"
                value={method}
                onChange={(e) => setMethod(METHODS.find((m) => m === e.target.value) ?? "cash")}
              >
                {METHODS.map((m) => (
                  <option key={m} value={m}>
                    {t(`payments:tenders.${m}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            {method !== "cash" ? (
              <Field id="order-reference" label={t("orders:dialog.reference")}>
                <GuardedInput
                  id="order-reference"
                  dir="ltr"
                  maxLength={40}
                  value={reference}
                  onValueChange={setReference}
                />
              </Field>
            ) : null}
          </div>
          {layaway ? (
            <p className={belowMinimum ? "font-semibold text-danger" : "text-ink-2"}>
              {minimum === null
                ? t("common:status.loading")
                : t("orders:dialog.minimum", { amount: formatDZD(minimum, lang) })}
            </p>
          ) : null}
          {deposit && quote.data ? (
            <div role="status" className="flex flex-col gap-1 rounded-card bg-surface-2 p-4">
              {quote.data.stampCharged > 0 ? (
                <p>{t("orders:dialog.duty", { amount: formatDZD(quote.data.stampCharged, lang) })}</p>
              ) : null}
              <p className="text-lg font-bold">
                {t("orders:dialog.toReceive", { amount: formatDZD(quote.data.toReceive, lang) })}
              </p>
            </div>
          ) : null}
          <Field id="order-note" label={t("orders:dialog.note")} optional={t("orders:optional")}>
            <Input
              id="order-note"
              dir="auto"
              maxLength={200}
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </Field>
          {problem ? (
            <p role="alert" className="text-danger">
              {errorText(problem)}
            </p>
          ) : null}
        </form>
      </Dialog>
      {approval.dialog}
    </>
  );
}
