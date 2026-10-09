import { Plus, Trash2 } from "lucide-react";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { type Picked, ProductPicker } from "@/features/purchases/purchase-lines";
import { formatAmountInput } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import { useSession } from "@/lib/ipc/auth";
import { useProduct } from "@/lib/ipc/products";
import type { CustomerPrice } from "@/lib/ipc/types";
import { useCustomerPrices, useSetCustomerPrice } from "@/lib/ipc/wholesale";
import { normalizeDigits, parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** U+2212 MINUS SIGN, U+00A0 NO-BREAK SPACE (before "%" in French). */
const MINUS = "\u2212";
const NBSP = "\u00A0";

/**
 * How far the agreed price stands from the list price, `(agreed − list) / list` with one decimal
 * ("−12,5 %"): a ratio shown beside two prices the backend gave, never an amount. Integer
 * arithmetic, rounded half away from zero; "—" without a list price.
 */
export function priceGap(agreed: number, list: number, lang: Lang): string {
  if (list <= 0) return "—";
  const diff = agreed - list;
  const tenths = Math.floor((Math.abs(diff) * 1_000 * 2 + list) / (2 * list));
  const sign = tenths === 0 ? "" : diff < 0 ? MINUS : "+";
  const number = `${sign}${Math.floor(tenths / 10)}${lang === "en" ? "." : ","}${tenths % 10}`;
  return lang === "fr" ? `${number}${NBSP}%` : `${number}%`;
}

/**
 * PRC-003, the customer's "Prix client" tab: the prices agreed with him (product, reference, list
 * price, agreed price, difference), each agreed price edited in place and saved when the field is
 * left, removed, or added by product search. Editing needs `products.price` (the backend checks
 * it again); the prices are the main PC's, so a till's change goes there.
 */
export function CustomerPrices({ customerId }: { customerId: string }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const permissions = useSession().data?.permissions ?? [];
  const canEdit = permissions.includes("products.price");
  const prices = useCustomerPrices(customerId);
  const [search, setSearch] = useState("");
  const [adding, setAdding] = useState(false);
  const text = normalizeDigits(search.trim()).toLocaleLowerCase();
  const rows = (prices.data ?? []).filter(
    (p) =>
      text === "" ||
      p.productName.toLocaleLowerCase().includes(text) ||
      (p.reference ?? "").toLocaleLowerCase().includes(text),
  );

  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-ink-2">{t("wholesale:prices.hint")}</p>
      <div className="flex flex-wrap items-end gap-3">
        <Field id="customer-prices-search" label={t("tables:search")}>
          <Input
            id="customer-prices-search"
            type="search"
            dir="auto"
            className="min-w-60"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </Field>
        {canEdit && !adding ? (
          <Button variant="outline" onClick={() => setAdding(true)}>
            <Plus aria-hidden />
            {t("wholesale:prices.add")}
          </Button>
        ) : null}
      </div>
      {adding ? <AddPrice customerId={customerId} onDone={() => setAdding(false)} /> : null}
      {prices.error ? (
        <p role="alert" className="text-danger">
          {errorText(prices.error)}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-card border border-line bg-surface">
          <table aria-label={t("wholesale:prices.label")} className="w-full border-collapse">
            <thead className="bg-surface-2">
              <tr className="border-b border-line text-sm text-ink-2">
                <th scope="col" className="px-3 py-2 text-start font-semibold">
                  {t("wholesale:prices.product")}
                </th>
                <th scope="col" className="px-3 py-2 text-start font-semibold">
                  {t("wholesale:prices.reference")}
                </th>
                <th scope="col" className="px-3 py-2 text-end font-semibold">
                  {t("wholesale:prices.list")}
                </th>
                <th scope="col" className="px-3 py-2 text-end font-semibold">
                  {t("wholesale:prices.agreed")}
                </th>
                <th scope="col" className="px-3 py-2 text-end font-semibold">
                  {t("wholesale:prices.diff")}
                </th>
                {canEdit ? (
                  <th scope="col" className="px-3 py-2">
                    <span className="sr-only">{t("wholesale:prices.remove")}</span>
                  </th>
                ) : null}
              </tr>
            </thead>
            <tbody>
              {rows.length === 0 ? (
                <tr>
                  <td colSpan={canEdit ? 6 : 5} className="px-3 py-6 text-center text-ink-2">
                    {prices.isPending ? t("common:status.loading") : t("wholesale:prices.empty")}
                  </td>
                </tr>
              ) : (
                rows.map((price) => <PriceRow key={price.productId} price={price} canEdit={canEdit} />)
              )}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function PriceRow({ price, canEdit }: { price: CustomerPrice; canEdit: boolean }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const setPrice = useSetCustomerPrice();
  const [editing, setEditing] = useState<string | null>(null);
  const [invalid, setInvalid] = useState(false);
  const product = price.productName;
  const gap = price.price - price.listPrice;

  // Saved when the field is left: unchanged or empty keeps the stored price.
  const save = () => {
    if (editing === null) return;
    const typed = editing.trim();
    const value = parseAmount(typed);
    if (typed === "" || value === price.price) {
      setEditing(null);
      return;
    }
    if (value === null || value <= 0) {
      setInvalid(true);
      return;
    }
    setPrice.mutate(
      { customerId: price.customerId, productId: price.productId, price: value },
      {
        onSuccess: () => {
          setEditing(null);
          toast.success(t("wholesale:prices.saved", { product }));
        },
        onError: (error) => toast.error(errorText(error)),
      },
    );
  };
  const remove = () =>
    setPrice.mutate(
      { customerId: price.customerId, productId: price.productId, price: null },
      {
        onSuccess: () => toast.success(t("wholesale:prices.removed", { product })),
        onError: (error) => toast.error(errorText(error)),
      },
    );

  return (
    <tr className="border-b border-line even:bg-surface-2">
      <td className="px-3 py-2" dir="auto">
        {product}
      </td>
      <td className="px-3 py-2">{price.reference ? <bdi dir="ltr">{price.reference}</bdi> : null}</td>
      <td className="px-3 py-2 text-end">
        <Amount value={price.listPrice} />
      </td>
      <td className="px-3 py-2 text-end">
        {editing !== null ? (
          <NumericInput
            autoFocus
            aria-label={t("wholesale:prices.edit", { product })}
            aria-invalid={invalid}
            className="ms-auto w-36 text-end"
            value={editing}
            disabled={setPrice.isPending}
            onValueChange={(value) => {
              setEditing(value);
              setInvalid(false);
            }}
            onBlur={save}
            onKeyDown={(event) => {
              if (event.key === "Enter") event.currentTarget.blur();
              if (event.key === "Escape") {
                event.preventDefault();
                setEditing(null);
                setInvalid(false);
              }
            }}
          />
        ) : canEdit ? (
          <button
            type="button"
            aria-label={t("wholesale:prices.edit", { product })}
            className="min-h-11 rounded-control px-2 font-semibold hover:bg-surface-2 focus-visible:outline-3 focus-visible:outline-focus"
            onClick={() => setEditing(formatAmountInput(price.price, lang))}
          >
            <Amount value={price.price} />
          </button>
        ) : (
          <Amount value={price.price} className="font-semibold" />
        )}
        {invalid ? <p className="text-sm text-danger">{t("validation:amountFormat")}</p> : null}
      </td>
      <td className={cn("px-3 py-2 text-end tabular-nums", gap < 0 ? "text-ok" : gap > 0 ? "text-warn" : "")}>
        <bdi dir="ltr">{priceGap(price.price, price.listPrice, lang)}</bdi>
      </td>
      {canEdit ? (
        <td className="px-3 py-2 text-end">
          <Button
            variant="ghost"
            size="icon"
            aria-label={t("wholesale:prices.removeOf", { product })}
            disabled={setPrice.isPending}
            onClick={remove}
          >
            <Trash2 aria-hidden />
          </Button>
        </td>
      ) : null}
    </tr>
  );
}

/** A product found by search (or scanned), then its agreed price: the list price to start from. */
function AddPrice({ customerId, onDone }: { customerId: string; onDone: () => void }) {
  const { t } = useTranslation();
  const [picked, setPicked] = useState<Picked | null>(null);
  return (
    <section
      aria-label={t("wholesale:prices.add")}
      className="flex flex-col gap-3 rounded-card border border-line p-4"
    >
      {picked ? (
        <PriceOf key={picked.productId} customerId={customerId} picked={picked} onDone={onDone} />
      ) : (
        <ProductPicker autoFocus onPick={setPicked} />
      )}
      <Button variant="ghost" className="self-start" onClick={onDone}>
        {t("common:actions.cancel")}
      </Button>
    </section>
  );
}

function PriceOf({ customerId, picked, onDone }: { customerId: string; picked: Picked; onDone: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const product = useProduct(picked.productId);
  const setPrice = useSetCustomerPrice();
  const [typed, setTyped] = useState<string | null>(null);
  const [invalid, setInvalid] = useState(false);
  const listPrice = product.data?.sellPrice;
  const value = typed ?? (listPrice !== undefined ? formatAmountInput(listPrice, lang) : "");
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const price = parseAmount(value);
    if (price === null || price <= 0) {
      setInvalid(true);
      return;
    }
    setPrice.mutate(
      { customerId, productId: picked.productId, price },
      {
        onSuccess: () => {
          toast.success(t("wholesale:prices.saved", { product: picked.name }));
          onDone();
        },
      },
    );
  };
  return (
    <form noValidate onSubmit={submit} className="flex flex-wrap items-end gap-3">
      <Field
        id="customer-price-new"
        label={t("wholesale:prices.for", { product: picked.name })}
        error={
          invalid ? t("validation:amountFormat") : setPrice.error ? errorText(setPrice.error) : undefined
        }
      >
        <NumericInput
          id="customer-price-new"
          autoFocus
          className="w-40"
          value={value}
          onValueChange={(v) => {
            setTyped(v);
            setInvalid(false);
          }}
        />
      </Field>
      {listPrice !== undefined ? (
        <p className="flex min-h-11 items-center gap-2 text-ink-2">
          {t("wholesale:prices.list")}
          <Amount value={listPrice} />
        </p>
      ) : null}
      <Button type="submit" variant="primary" disabled={setPrice.isPending}>
        {t("wholesale:prices.save")}
      </Button>
    </form>
  );
}
