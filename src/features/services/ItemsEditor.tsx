import { Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { NumericInput } from "@/components/numeric-input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formatAmountInput, formatDZD } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import { useProduct, useProductSearch } from "@/lib/ipc/products";
import { useSetServiceItems } from "@/lib/ipc/services";
import type { ProductHit, ServiceItemInput, ServiceTicket } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty, normalizeDigits, parseAmount, parseQty } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** A line as edited: a catalogue part sells at its catalogue price (`price === null`); labour, a
 * service whose price is typed, carries it. */
interface Line {
  key: number;
  productId: string;
  name: string;
  qty: string;
  price: string | null;
}

let nextKey = 1;

function linesOf(ticket: ServiceTicket, lang: Lang): Line[] {
  return ticket.items.map((item) => ({
    key: nextKey++,
    productId: item.productId,
    name: item.name,
    qty: formatQty(item.qty, lang),
    price: item.unitPrice === null ? null : formatAmountInput(item.unitPrice, lang),
  }));
}

/**
 * The parts and labour planned on a ticket, sold at the pickup: parts searched in the catalogue
 * (their catalogue price), labour as a service line whose price is typed, quantities. "Enregistrer
 * les pièces" replaces the list (`service_set_items`); no total is computed here (the pickup's
 * ticket is priced by the backend).
 */
export function ItemsEditor({ ticket, editable }: { ticket: ServiceTicket; editable: boolean }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const save = useSetServiceItems();
  const [lines, setLines] = useState<Line[]>(() => linesOf(ticket, lang));
  const [search, setSearch] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const query = useDebounced(normalizeDigits(search.trim()), 200);
  const hits = useProductSearch(
    { text: query, limit: 8, categoryId: null, includeArchived: false },
    editable,
  );
  const shown = search.trim() === "" ? [] : (hits.data ?? []).filter((h) => h.kind !== "variant_parent");

  const add = (hit: ProductHit) => {
    setLines((all) => [
      ...all,
      {
        key: nextKey++,
        productId: hit.id,
        name: localizedName(hit, lang),
        qty: "1",
        // labour (a service) carries the price agreed with the customer
        price: hit.kind === "service" ? formatAmountInput(hit.sellPrice, lang) : null,
      },
    ]);
    setSearch("");
  };
  const update = (key: number, change: Partial<Line>) =>
    setLines((all) => all.map((l) => (l.key === key ? { ...l, ...change } : l)));

  const submit = () => {
    const items: ServiceItemInput[] = [];
    for (const line of lines) {
      const qty = parseQty(line.qty, 3);
      const unitPrice = line.price === null ? null : parseAmount(line.price);
      if (qty === null) {
        setProblem(t("validation:qtyFormat"));
        return;
      }
      if (line.price !== null && (unitPrice === null || unitPrice < 0)) {
        setProblem(t("validation:amountFormat"));
        return;
      }
      items.push({ productId: line.productId, qty, unitPrice });
    }
    setProblem(null);
    save.mutate({ id: ticket.id, items }, { onSuccess: () => toast.success(t("services:items.saved")) });
  };

  return (
    <section aria-label={t("services:items.title")} className="flex flex-col gap-3">
      <h2 className="text-xl font-bold">{t("services:items.title")}</h2>
      {lines.length === 0 ? <p className="text-ink-2">{t("services:items.empty")}</p> : null}
      <ul className="flex flex-col gap-2">
        {lines.map((line) => (
          <li
            key={line.key}
            className="flex flex-wrap items-end gap-3 rounded-control border border-line p-3"
          >
            <div className="flex min-w-48 flex-1 flex-col gap-1">
              <span className="font-semibold">{line.name}</span>
              {line.price === null ? (
                <CatalogPrice productId={line.productId} />
              ) : (
                <Badge tone="info" className="self-start">
                  {t("services:items.labour")}
                </Badge>
              )}
            </div>
            <div className="w-28">
              <Field id={`item-qty-${line.key}`} label={t("services:items.qty")}>
                <NumericInput
                  id={`item-qty-${line.key}`}
                  kind="qty"
                  disabled={!editable}
                  value={line.qty}
                  onValueChange={(qty) => update(line.key, { qty })}
                />
              </Field>
            </div>
            {line.price !== null ? (
              <div className="w-36">
                <Field id={`item-price-${line.key}`} label={t("services:items.price")}>
                  <NumericInput
                    id={`item-price-${line.key}`}
                    disabled={!editable}
                    value={line.price}
                    onValueChange={(price) => update(line.key, { price })}
                  />
                </Field>
              </div>
            ) : null}
            {editable ? (
              <Button
                variant="ghost"
                size="icon"
                aria-label={t("services:items.remove", { name: line.name })}
                title={t("services:items.remove", { name: line.name })}
                onClick={() => setLines((all) => all.filter((l) => l.key !== line.key))}
              >
                <Trash2 aria-hidden />
              </Button>
            ) : null}
          </li>
        ))}
      </ul>
      {editable ? (
        <>
          <Field id="item-search" label={t("services:items.search")}>
            <div className="relative">
              <Input
                id="item-search"
                type="search"
                dir="auto"
                autoComplete="off"
                value={search}
                onChange={(event) => setSearch(event.target.value)}
              />
              {shown.length > 0 ? (
                <ul
                  aria-label={t("services:items.add")}
                  className="absolute inset-x-0 top-full z-20 mt-1 flex flex-col rounded-control border border-line bg-surface py-1 shadow-overlay"
                >
                  {shown.map((hit) => (
                    <li key={hit.id}>
                      <button
                        type="button"
                        onClick={() => add(hit)}
                        className="flex min-h-11 w-full items-center gap-2 px-3 text-start hover:bg-surface-2"
                      >
                        <span className="truncate">{localizedName(hit, lang)}</span>
                        <Amount value={hit.sellPrice} className="ms-auto text-sm text-ink-2" />
                      </button>
                    </li>
                  ))}
                </ul>
              ) : null}
            </div>
          </Field>
          {problem ? (
            <p role="alert" className="text-danger">
              {problem}
            </p>
          ) : null}
          {save.error ? (
            <p role="alert" className="text-danger">
              {errorText(save.error)}
            </p>
          ) : null}
          <Button variant="secondary" className="self-start" disabled={save.isPending} onClick={submit}>
            {t("services:items.save")}
          </Button>
        </>
      ) : null}
    </section>
  );
}

/** A part's price as the catalogue says (the pickup sells it at the price of that day). */
function CatalogPrice({ productId }: { productId: string }) {
  const { t } = useTranslation();
  const lang = useLang();
  const product = useProduct(productId);
  if (!product.data) return null;
  return (
    <span className="text-sm text-ink-2">
      {t("services:items.catalogPrice", { amount: formatDZD(product.data.sellPrice, lang) })}
    </span>
  );
}
