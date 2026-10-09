import { X } from "lucide-react";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Sheet } from "@/components/ui/sheet";
import { BuyerFlag } from "@/features/orders/BuyerFlag";
import { toAsciiDigits } from "@/lib/digits";
import { useCustomer, useCustomerSearch, useLastTicket, useQuickCreateCustomer } from "@/lib/ipc/customers";
import type { Customer, CustomerHit, Session } from "@/lib/ipc/types";
import { normalizeDigits } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { cn } from "@/lib/utils";
import { useCart } from "./cart-store";
import type { PosContext } from "./PosDialogs";

/** SAL-037: the balance, the limit and what is left to sell on credit, as the backend computed
 * them; only for users holding `credit.view` (the others get zeros from the backend). */
export function CreditFigures({ customer }: { customer: Customer | CustomerHit }) {
  const { t } = useTranslation();
  return (
    <span className="flex flex-wrap gap-x-3 text-sm">
      <span>
        {t("customers:detail.balance")} <Amount value={customer.balance} className="font-bold" />
      </span>
      {customer.creditLimit === null ? (
        <span>{t("customers:noLimit")}</span>
      ) : (
        <>
          <span>
            {t("customers:detail.limit")} <Amount value={customer.creditLimit} />
          </span>
          {customer.availableCredit !== null ? (
            <span>
              {t("customers:detail.available")} <Amount value={customer.availableCredit} />
            </span>
          ) : null}
        </>
      )}
      {customer.storeCredit > 0 ? (
        <span className="text-ok">
          {t("customers:detail.storeCredit")} <Amount value={customer.storeCredit} />
        </span>
      ) : null}
    </span>
  );
}

/** Name, code and, when his credit is blocked (CRD-002), a badge saying so. */
export function CustomerName({ customer }: { customer: Customer | CustomerHit }) {
  const { t } = useTranslation();
  return (
    <span className="flex flex-wrap items-center gap-2">
      <span className="font-semibold">{customer.name}</span>
      <bdi dir="ltr" className="text-sm text-ink-2">
        {customer.code}
      </bdi>
      {customer.isBlocked ? <Badge tone="danger">{t("customers:status.blocked")}</Badge> : null}
    </span>
  );
}

/** The ticket's customer under the cart title: tap to change him, × to sell without him. */
export function CustomerStrip({ session, onOpen }: { session: Session; onOpen: () => void }) {
  const { t } = useTranslation();
  const customerId = useCart((s) => s.customerId);
  const customer = useCustomer(customerId);
  if (customerId === null) return null;
  const seesCredit = session.permissions.includes("credit.view");
  return (
    <div
      role="group"
      aria-label={t("pos:customer.strip")}
      className="flex items-center gap-2 border-b border-line bg-info-soft px-4 py-2"
    >
      <button type="button" onClick={onOpen} className="flex min-w-0 flex-1 flex-col gap-1 text-start">
        {customer.data ? (
          <>
            <CustomerName customer={customer.data} />
            {seesCredit ? <CreditFigures customer={customer.data} /> : null}
          </>
        ) : (
          <span aria-hidden>…</span>
        )}
      </button>
      <Button
        variant="ghost"
        size="icon"
        aria-label={t("pos:customer.remove")}
        title={t("pos:customer.remove")}
        onClick={() => useCart.getState().setCustomer(null)}
      >
        <X aria-hidden />
      </Button>
    </div>
  );
}

/** F3 (CRM-001/003, SAL-029, SAL-037): search by name, phone or code (Arabic-Indic digits
 * read as 0-9), quick creation with a name and a phone, the chosen customer's account and his
 * last sale rung up again. */
export function CustomerPicker({ ctx, onClose }: { ctx: PosContext; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const seesCredit = ctx.session.permissions.includes("credit.view");
  const customerId = useCart((s) => s.customerId);
  const current = useCustomer(customerId);
  const [text, setText] = useState("");
  const [draft, setDraft] = useState<{ name: string; phone: string } | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [noLastSale, setNoLastSale] = useState(false);
  const query = useDebounced(normalizeDigits(text.trim()), 150);
  const hits = useCustomerSearch({ text: query, limit: 20 }, draft === null);
  const quick = useQuickCreateCustomer();
  const lastTicket = useLastTicket();

  const choose = (id: string) => {
    useCart.getState().setCustomer(id);
    onClose();
  };

  const startDraft = () => {
    const typed = text.trim();
    // what was typed becomes the phone when it is one, the name otherwise
    const phone = /^[\d\s+]+$/.test(normalizeDigits(typed));
    setError(null);
    setDraft({ name: phone ? "" : typed, phone: phone ? typed : "" });
  };

  const create = (event: FormEvent) => {
    event.preventDefault();
    if (!draft) return;
    const phone = toAsciiDigits(draft.phone.trim());
    quick.mutate(
      { name: draft.name.trim(), phone: phone === "" ? null : phone },
      { onSuccess: (created) => choose(created.id), onError: setError },
    );
  };

  // SAL-029: the lines of his last ticket, priced again by the backend (catalogue items only:
  // an open item has no product to price)
  const repeat = (id: string, name: string) => {
    setError(null);
    setNoLastSale(false);
    lastTicket.mutate(id, {
      onSuccess: (doc) => {
        if (!doc) {
          setNoLastSale(true);
          return;
        }
        const lines = doc.lines.filter((l) => l.parentLineId === null && l.qty > 0);
        const items = lines.flatMap((l) =>
          l.productId === null
            ? []
            : [
                {
                  productId: l.productId,
                  qty: l.qty,
                  display: {
                    name: l.description,
                    unitCode: l.unitCode,
                    unitDecimals: l.unitCode === "pc" ? 0 : 3,
                    price: l.unitPrice,
                  },
                },
              ],
        );
        useCart.getState().addLines(items);
        const skipped = items.length < lines.length;
        ctx.notify(
          "info",
          t(skipped ? "pos:customer.repeatedOpenItems" : "pos:customer.repeated", {
            name,
            number: doc.number ?? "",
          }),
        );
        onClose();
      },
      onError: setError,
    });
  };

  const results = hits.data ?? [];
  return (
    <Sheet
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("pos:customer.title")}
      onEscapeKeyDown={(event) => {
        // Esc clears first: the new-customer form, then the search; closes after
        if (draft !== null) {
          event.preventDefault();
          setDraft(null);
        } else if (text !== "") {
          event.preventDefault();
          setText("");
        }
      }}
    >
      <div className="flex flex-col gap-4">
        {current.data ? (
          <section
            aria-label={t("pos:customer.strip")}
            className="flex flex-col gap-2 rounded-card border border-brand bg-brand-soft p-3"
          >
            <CustomerName customer={current.data} />
            {seesCredit ? <CreditFigures customer={current.data} /> : null}
            {/* DZB-015: DZBuild's flag on his number, a warning only (nothing while unlinked) */}
            <BuyerFlag phone={current.data.phone} />
            <div className="flex flex-wrap gap-2">
              <Button
                variant="outline"
                disabled={lastTicket.isPending}
                onClick={() => current.data && repeat(current.data.id, current.data.name)}
              >
                {t("pos:customer.repeat")}
              </Button>
              <Button variant="ghost" onClick={() => useCart.getState().setCustomer(null)}>
                {t("pos:customer.remove")}
              </Button>
            </div>
          </section>
        ) : null}

        {draft === null ? (
          <>
            <Field id="customer-search" label={t("pos:customer.search")} hint={t("pos:customer.searchHint")}>
              <Input
                id="customer-search"
                type="search"
                dir="auto"
                autoFocus
                autoComplete="off"
                value={text}
                onChange={(e) => setText(e.target.value)}
              />
            </Field>
            <ul aria-label={t("pos:customer.results")} className="flex flex-col gap-2">
              {results.map((hit) => (
                <li key={hit.id}>
                  <button
                    type="button"
                    onClick={() => choose(hit.id)}
                    className={cn(
                      "flex min-h-14 w-full flex-col gap-1 rounded-card border p-3 text-start hover:bg-surface-2",
                      hit.id === customerId ? "border-brand" : "border-line",
                    )}
                  >
                    <CustomerName customer={hit} />
                    {hit.phone ? (
                      <bdi dir="ltr" className="text-sm text-ink-2">
                        {hit.phone}
                      </bdi>
                    ) : null}
                    {seesCredit ? <CreditFigures customer={hit} /> : null}
                  </button>
                </li>
              ))}
            </ul>
            {hits.isSuccess && results.length === 0 ? (
              <p className="text-ink-2">{t("pos:customer.none")}</p>
            ) : null}
            <div>
              <Button variant="outline" onClick={startDraft}>
                {t("customers:new")}
              </Button>
            </div>
          </>
        ) : (
          <form noValidate onSubmit={create} className="flex flex-col gap-3">
            <p className="font-semibold">{t("customers:new")}</p>
            <Field id="quick-name" label={t("customers:fields.name")}>
              <Input
                id="quick-name"
                autoFocus
                required
                maxLength={120}
                value={draft.name}
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              />
            </Field>
            <Field
              id="quick-phone"
              label={t("customers:fields.phone")}
              optional={t("customers:form.optional")}
            >
              <Input
                id="quick-phone"
                dir="ltr"
                inputMode="tel"
                maxLength={20}
                value={draft.phone}
                onChange={(e) => setDraft({ ...draft, phone: e.target.value })}
              />
            </Field>
            <div className="flex flex-wrap gap-2">
              <Button type="submit" variant="primary" disabled={quick.isPending || draft.name.trim() === ""}>
                {t("customers:form.save")}
              </Button>
              <Button variant="ghost" onClick={() => setDraft(null)}>
                {t("common:actions.cancel")}
              </Button>
            </div>
          </form>
        )}
        {noLastSale ? <p className="text-ink-2">{t("pos:customer.noLastSale")}</p> : null}
        {error !== null ? (
          <p role="alert" className="text-danger">
            {errorText(error)}
          </p>
        ) : null}
      </div>
    </Sheet>
  );
}
