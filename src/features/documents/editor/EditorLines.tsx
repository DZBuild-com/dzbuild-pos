import { createColumnHelper, tableFeatures, useTable } from "@tanstack/react-table";
import { Plus, Trash2 } from "lucide-react";
import { type KeyboardEvent, useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useStore, type StoreApi } from "zustand";
import { Amount } from "@/components/amount";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { ScanInput } from "@/components/scan-input";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { OpenItemDialog } from "@/features/pos/OpenItemDialog";
import { formatAmountInput, formatDZD, formatPercentInput, parsePercentBp } from "@/lib/format";
import { useFindByCode, useProductSearch } from "@/lib/ipc/products";
import type { Lang } from "@/lib/i18n";
import type { Discount, PricedLine, ProductHit, ScanHit } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty, normalizeDigits, parseAmount, parseQty } from "@/lib/numeric";
import { beep } from "@/lib/sounds";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import type { EditorLine, EditorState } from "./editor-store";

interface Row {
  line: EditorLine;
  /** The backend's pricing of the line, once it answered for these lines. */
  priced: PricedLine | undefined;
}

const features = tableFeatures({});
const helper = createColumnHelper<typeof features, Row>();

/** Keys typed in a field of a row stay in the field (the row takes Delete). */
function keepKeys(event: KeyboardEvent<HTMLElement>): void {
  event.stopPropagation();
}

/** "5 %" for a percentage, the amount with its currency for an amount off the line: what the
 * discount field shows. */
function discountText(discount: Discount, lang: Lang): string {
  if (discount.kind === "percent") return `${formatPercentInput(discount.value, lang)} %`;
  if (discount.kind === "amount") return formatDZD(discount.value, lang);
  return "";
}

/** A typed discount: a number is a percentage, a number followed by DA (دج) an amount off the
 * line; empty is none. `null` when unreadable. */
function parseDiscount(text: string): Discount | null {
  const typed = normalizeDigits(text).trim();
  if (typed === "") return { kind: "none" };
  if (/(DA|دج)$/iu.test(typed)) {
    const amount = parseAmount(typed);
    return amount === null ? null : amount === 0 ? { kind: "none" } : { kind: "amount", value: amount };
  }
  const bp = parsePercentBp(typed.replace(/\s*%$/, ""));
  return bp === null ? null : bp === 0 ? { kind: "none" } : { kind: "percent", value: bp };
}

/** A field of a line typed in place: it keeps what is typed and hands it over on Enter or when
 * the focus leaves; `onCommit` answers whether the text was taken (else it shows as invalid). */
function CellField({
  label,
  initial,
  placeholder,
  kind,
  dir = "auto",
  className,
  onCommit,
}: {
  label: string;
  initial: string;
  placeholder?: string;
  kind: "qty" | "amount" | "text";
  /** A text field's direction: a discount reads left to right like every number. */
  dir?: "auto" | "ltr";
  className: string;
  onCommit: (text: string) => boolean;
}) {
  const [text, setText] = useState(initial);
  const [invalid, setInvalid] = useState(false);
  const commit = () => {
    if (text !== initial) setInvalid(!onCommit(text));
  };
  const common = {
    "aria-label": label,
    "aria-invalid": invalid,
    placeholder,
    onBlur: commit,
    onKeyDown: (event: KeyboardEvent<HTMLInputElement>) => {
      keepKeys(event);
      if (event.key === "Enter") {
        event.preventDefault();
        commit();
      }
    },
    className: `h-10 min-h-10 ${className}`,
  };
  const change = (value: string) => {
    setText(value);
    setInvalid(false);
  };
  return kind === "text" ? (
    <GuardedInput {...common} dir={dir} maxLength={200} value={text} onValueChange={change} />
  ) : (
    <NumericInput {...common} kind={kind} value={text} onValueChange={change} />
  );
}

/**
 * The lines of a document being typed (DOC-001): a scan or a search adds a product (the same one
 * again adds to its quantity), "Article libre" an item without a catalogue entry; quantity, unit
 * price (empty: the catalogue's), discount and note are typed in place, Delete removes the
 * focused line. Every price and total is the backend's.
 */
export function EditorLines({
  store,
  priced,
  ht,
}: {
  store: StoreApi<EditorState>;
  priced: readonly PricedLine[] | undefined;
  /** Totals shown HT (the document's price mode). */
  ht: boolean;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const lines = useStore(store, (s) => s.lines);
  const [openItem, setOpenItem] = useState(false);
  const data = useMemo<Row[]>(
    () =>
      lines.map((line, index) => {
        const candidate = priced?.[index];
        // a figure is shown only once the backend priced this very line (not the one before an
        // insertion or a removal)
        const same =
          candidate !== undefined &&
          candidate.productId === line.line.productId &&
          (line.line.productId !== null || candidate.description === line.line.description);
        return { line, priced: same ? candidate : undefined };
      }),
    [lines, priced],
  );

  const columns = useMemo(() => {
    const named = (field: string, name: string) => t("billing:editor.lineField", { field, name });
    const nameOf = (row: Row) => row.priced?.description ?? row.line.name;
    const actions = store.getState();
    return helper.columns([
      helper.display({
        id: "product",
        header: () => t("billing:editor.columns.product"),
        cell: ({ row }) => {
          const { line, priced: p } = row.original;
          const name = nameOf(row.original);
          return (
            <div className="flex flex-col gap-1 py-1">
              <span className="font-semibold">{name}</span>
              {p?.priceChanged ? (
                <span className="text-sm text-warn">{t("pos:cart.priceChanged")}</span>
              ) : null}
              <CellField
                key={line.line.note ?? ""}
                kind="text"
                label={named(t("billing:editor.note"), name)}
                placeholder={t("billing:editor.note")}
                initial={line.line.note ?? ""}
                className="w-full max-w-72 text-sm"
                onCommit={(text) => {
                  actions.setLineNote(line.key, text.trim() === "" ? null : text.trim());
                  return true;
                }}
              />
            </div>
          );
        },
      }),
      helper.display({
        id: "qty",
        header: () => t("billing:editor.columns.qty"),
        cell: ({ row }) => {
          const { line, priced: p } = row.original;
          const decimals = (p?.unitCode ?? line.unitCode) === "pc" ? 0 : 3;
          return (
            <CellField
              key={line.line.qty}
              kind="qty"
              label={named(t("billing:editor.columns.qty"), nameOf(row.original))}
              initial={formatQty(line.line.qty, lang)}
              className="w-24 text-end"
              onCommit={(text) => {
                const qty = parseQty(text, decimals);
                if (qty === null) return false;
                actions.setQty(line.key, qty);
                return true;
              }}
            />
          );
        },
      }),
      helper.display({
        id: "unitPrice",
        header: () => t("billing:editor.columns.unitPrice"),
        cell: ({ row }) => {
          const { line, priced: p } = row.original;
          const own = line.line.unitPrice;
          return (
            <CellField
              key={own ?? "catalogue"}
              kind="amount"
              label={named(t("billing:editor.columns.unitPrice"), nameOf(row.original))}
              initial={own === null ? "" : formatAmountInput(own, lang)}
              placeholder={p ? formatAmountInput(p.unitPrice, lang) : undefined}
              className="w-32 text-end"
              onCommit={(text) => {
                // empty: the catalogue's price again (an open item always has its own)
                if (text.trim() === "") {
                  if (line.line.productId === null) return false;
                  actions.setUnitPrice(line.key, null);
                  return true;
                }
                const price = parseAmount(text);
                if (price === null) return false;
                actions.setUnitPrice(line.key, price);
                return true;
              }}
            />
          );
        },
      }),
      helper.display({
        id: "discount",
        header: () => t("billing:editor.columns.discount"),
        cell: ({ row }) => {
          const { line } = row.original;
          const shown = discountText(line.line.discount, lang);
          return (
            <CellField
              key={shown}
              kind="text"
              dir="ltr"
              label={named(t("billing:editor.columns.discount"), nameOf(row.original))}
              placeholder="%"
              initial={shown}
              className="w-28 text-end"
              onCommit={(text) => {
                const discount = parseDiscount(text);
                if (discount === null) return false;
                actions.setDiscount(line.key, discount);
                return true;
              }}
            />
          );
        },
      }),
      helper.display({
        id: "total",
        header: () => t("billing:editor.columns.total"),
        cell: ({ row }) => {
          const p = row.original.priced;
          return p ? (
            <Amount value={ht ? p.amounts.totalHt : p.amounts.totalTtc} className="font-semibold" />
          ) : (
            <span aria-label={t("pos:cart.pricing")}>…</span>
          );
        },
      }),
      helper.display({
        id: "remove",
        header: () => <span className="sr-only">{t("billing:editor.removeLine")}</span>,
        cell: ({ row }) => (
          <Button
            variant="ghost"
            size="icon"
            aria-label={named(t("billing:editor.removeLine"), nameOf(row.original))}
            title={t("billing:editor.removeLine")}
            onClick={() => actions.remove(row.original.line.key)}
          >
            <Trash2 aria-hidden />
          </Button>
        ),
      }),
    ]);
  }, [t, lang, ht, store]);
  const table = useTable({ features, columns, data });
  const end = (id: string) => id === "qty" || id === "unitPrice" || id === "discount" || id === "total";

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-start gap-3">
        <div className="min-w-72 flex-1">
          <ProductSearch
            onPick={(item) => store.getState().addProduct(item)}
            onProblem={(error) => errorText(error)}
          />
        </div>
        <Button variant="outline" className="mt-7" onClick={() => setOpenItem(true)}>
          <Plus aria-hidden />
          {t("billing:editor.openItem")}
        </Button>
      </div>
      <div className="overflow-x-auto rounded-card border border-line bg-surface">
        <table className="w-full border-collapse">
          <thead className="bg-surface-2 text-sm text-ink-2">
            {table.getHeaderGroups().map((group) => (
              <tr key={group.id} className="h-10">
                {group.headers.map((header) => (
                  <th
                    key={header.id}
                    className={
                      end(header.column.id) ? "px-3 text-end font-semibold" : "px-3 text-start font-semibold"
                    }
                  >
                    {header.isPlaceholder ? null : <table.FlexRender header={header} />}
                  </th>
                ))}
              </tr>
            ))}
          </thead>
          <tbody>
            {data.length === 0 ? (
              <tr>
                <td colSpan={6} className="p-4 text-ink-2">
                  {t("billing:editor.empty")}
                </td>
              </tr>
            ) : null}
            {table.getRowModel().rows.map((row) => (
              <tr
                key={row.original.line.key}
                tabIndex={0}
                onKeyDown={(event) => {
                  if (event.key !== "Delete") return;
                  event.preventDefault();
                  store.getState().remove(row.original.line.key);
                }}
                className="border-t border-line align-top even:bg-surface-2 focus-visible:outline-3 focus-visible:-outline-offset-3 focus-visible:outline-focus"
              >
                {row.getAllCells().map((cell) => (
                  <td key={cell.id} className={end(cell.column.id) ? "px-3 py-1 text-end" : "px-3 py-1"}>
                    <table.FlexRender cell={cell} />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {openItem ? (
        <OpenItemDialog
          onClose={() => setOpenItem(false)}
          onAdd={(item) => store.getState().addOpenItem(item)}
        />
      ) : null}
    </section>
  );
}

/** PUR-010's picker for a document: a scan (or a code typed and Enter) looks the barcode up, a
 * pack's barcode bringing its quantity; Enter on a name takes the first match. */
function ProductSearch({
  onPick,
  onProblem,
}: {
  onPick: (item: { productId: string; qty: number; name: string; unitCode: string }) => void;
  onProblem: (error: unknown) => string;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const id = useId();
  const [text, setText] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const search = useDebounced(text.trim(), 150);
  const searching = search.length >= 2 && !/^\d+$/.test(normalizeDigits(search));
  const hits = useProductSearch(
    { text: search, limit: 8, categoryId: null, includeArchived: false },
    searching,
  );
  const shown = searching && text.trim() !== "" ? (hits.data ?? []) : [];
  const find = useFindByCode();

  const add = (product: ProductHit | ScanHit["product"], qty: number) => {
    setText("");
    if (product.kind === "variant_parent") {
      beep("error");
      setProblem(t("pos:scan.variantParent"));
      return;
    }
    setProblem(null);
    onPick({ productId: product.id, qty, name: localizedName(product, lang), unitCode: product.unitCode });
  };
  // One promise per code: codes scanned in a quick row each add their product.
  const lookup = (value: string) => {
    const code = normalizeDigits(value.trim());
    setText("");
    if (code === "") return;
    find
      .mutateAsync(code)
      .then((hit) => {
        if (hit) add(hit.product, hit.packQty);
        else {
          beep("error");
          setProblem(t("pos:scan.notFound", { code }));
        }
      })
      .catch((error: unknown) => {
        beep("error");
        setProblem(onProblem(error));
      });
  };

  return (
    <div className="flex flex-col gap-2">
      <Field id={id} label={t("billing:editor.addProduct")} error={problem ?? undefined}>
        <ScanInput
          id={id}
          type="search"
          dir="auto"
          value={text}
          onChange={(event) => setText(event.target.value)}
          onScan={lookup}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            const first = shown[0];
            if (/^\d+$/.test(normalizeDigits(text.trim()))) lookup(text);
            else if (first) add(first, 1_000);
          }}
        />
      </Field>
      {shown.length > 0 ? (
        <ul aria-label={t("pos:scan.results")} className="flex flex-wrap gap-2">
          {shown.map((hit) => (
            <li key={hit.id}>
              <Button variant="outline" onClick={() => add(hit, 1_000)}>
                {localizedName(hit, lang)}
                {hit.reference ? (
                  <bdi dir="ltr" className="text-sm text-ink-2">
                    {hit.reference}
                  </bdi>
                ) : null}
              </Button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
