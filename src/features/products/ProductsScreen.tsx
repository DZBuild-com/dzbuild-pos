import { createColumnHelper, tableFeatures, useTable } from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { BadgePercent, PencilLine, Plus, Printer, Tag, Tags, TrendingUp } from "lucide-react";
import { type ReactNode, Suspense, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useSearchParams } from "react-router";
import { Amount } from "@/components/amount";
import { ExportMenu } from "@/components/data-table/export-menu";
import { ScanInput } from "@/components/scan-input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { ImportLink } from "@/features/imports/import-entities";
import { PrintLabelsDialog } from "@/features/labels/PrintLabelsDialog";
import {
  ChifaImport,
  ControlledRegister,
  DepartmentKeysEditor,
  RegulatedBoard,
} from "@/features/packs/pack-screens";
import { useCategories } from "@/lib/ipc/catalog";
import { useTillLink } from "@/lib/ipc/lan";
import { usePackState } from "@/lib/ipc/pack-state";
import { useProduct, useProductCatalog, useProductSearch } from "@/lib/ipc/products";
import type { ArchiveFilter, Module, Permission, ProductHit, Session } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { BulkEditDialog } from "./BulkEditDialog";
import { BulkPriceDialog } from "./BulkPriceDialog";
import { CategoriesDialog } from "./CategoriesDialog";
import { ProductDialog } from "./ProductDialog";

const features = tableFeatures({});
const helper = createColumnHelper<typeof features, ProductHit>();

const ARCHIVE_FILTERS: ArchiveFilter[] = ["active", "archived", "all"];

function shownBy(filter: ArchiveFilter, hit: ProductHit): boolean {
  return filter === "all" || hit.archived === (filter === "archived");
}

/** `set` with `id` ticked or not. */
function toggled(set: ReadonlySet<string>, id: string, on: boolean): ReadonlySet<string> {
  const next = new Set(set);
  if (on) next.add(id);
  else next.delete(id);
  return next;
}

type Tab = "catalogue" | "chifa" | "register" | "regulated" | "departments";

const PACK_PANELS: Record<Exclude<Tab, "catalogue">, () => ReactNode> = {
  chifa: () => <ChifaImport />,
  register: () => <ControlledRegister />,
  regulated: () => <RegulatedBoard />,
  departments: () => <DepartmentKeysEditor />,
};

/** `/products`: the catalogue, and the pages of the catalogue's modules while they are on (M8):
 * the Chifa list and the controlled register (pharmacy), the capped prices (regulated staples),
 * the department keys. The tab is kept in `?tab=`; each pack page is a chunk of its own. */
export function ProductsScreen({ session }: { session: Session }) {
  const { t } = useTranslation();
  const [params, setParams] = useSearchParams();
  const modules = usePackState().data?.modules ?? [];
  const can = (p: Permission) => session.permissions.includes(p);
  const on = (m: Module) => modules.includes(m);
  const tabs: Tab[] = [
    "catalogue",
    ...(on("pharmacy") && can("products.edit") ? (["chifa"] as const) : []),
    ...(on("pharmacy") && can("pharmacy.register") ? (["register"] as const) : []),
    ...(on("regulated_prices") ? (["regulated"] as const) : []),
    ...(on("department_keys") ? (["departments"] as const) : []),
  ];
  const requested = params.get("tab");
  const tab = tabs.find((k) => k === requested) ?? "catalogue";
  const label = (k: Tab) =>
    ({
      catalogue: t("products:tabs.catalogue"),
      chifa: t("pharmacy:tabs.chifa"),
      register: t("pharmacy:tabs.register"),
      regulated: t("products:regulated.tab"),
      departments: t("products:departments.tab"),
    })[k];
  const tablist =
    tabs.length > 1 ? (
      <div
        role="tablist"
        aria-label={t("products:tabs.label")}
        className="flex flex-wrap gap-2 border-b border-line pb-2"
      >
        {tabs.map((k) => (
          <button
            key={k}
            type="button"
            role="tab"
            id={`products-tab-${k}`}
            aria-selected={tab === k}
            aria-controls={k === "catalogue" ? undefined : `products-panel-${k}`}
            onClick={() => setParams({ tab: k }, { replace: true })}
            className={cn(
              "min-h-11 rounded-control px-4 font-semibold",
              tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
            )}
          >
            {label(k)}
          </button>
        ))}
      </div>
    ) : null;
  if (tab === "catalogue") return <CatalogueTab session={session} tabs={tablist} />;
  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-3xl font-bold">{t("products:title")}</h1>
      {tablist}
      <div role="tabpanel" id={`products-panel-${tab}`} aria-labelledby={`products-tab-${tab}`}>
        <Suspense fallback={null}>{PACK_PANELS[tab]()}</Suspense>
      </div>
    </div>
  );
}

/** The catalogue: every product, sold at the till or not, archived ones on demand, 100 at a time.
 * Ticked rows are changed together (PRD-066 bulk edit, PRC-006 bulk prices) or get their labels
 * (LBL-006); the promotions and the labels have their own pages. */
function CatalogueTab({ session, tabs }: { session: Session; tabs: ReactNode }) {
  const { t } = useTranslation();
  const lang = useLang();
  const [text, setText] = useState("");
  const [category, setCategory] = useState("");
  const [archived, setArchived] = useState<ArchiveFilter>("active");
  const [open, setOpen] = useState<string | "new" | null>(null);
  const [categoriesOpen, setCategoriesOpen] = useState(false);
  const [bulk, setBulk] = useState<"price" | "edit" | "labels" | null>(null);
  const [ticked, setTicked] = useState<ReadonlySet<string>>(new Set());
  const query = useDebounced(text.trim(), 200);
  const categories = (useCategories().data ?? []).filter((c) => !c.archived);
  const search = useProductSearch(
    { text: query, limit: 100, categoryId: category || null, includeArchived: true },
    query !== "",
  );
  const catalog = useProductCatalog({ categoryId: category || null, archived });
  const rows = useMemo(
    () =>
      query !== ""
        ? (search.data ?? []).filter((hit) => shownBy(archived, hit))
        : (catalog.data?.pages.flat() ?? []),
    [query, search.data, catalog.data, archived],
  );
  const selected = useProduct(open !== null && open !== "new" ? open : null);
  // On a till, products are edited on the main PC (routed there while it answers); categories
  // never are.
  const till = useTillLink();
  const canEdit = session.permissions.includes("products.edit") && (!till.isTill || till.online);
  const canPrice = session.permissions.includes("products.price");
  const canLabel = session.permissions.includes("products.labels");
  // IMX-010: the catalogue as the import reads it back (costs only with `reports.profit`).
  const canExport = session.permissions.includes("products.view");
  const selects = canEdit || canPrice || canLabel;
  const allTicked = rows.length > 0 && rows.every((r) => ticked.has(r.id));
  const columns = useMemo(
    () =>
      helper.columns([
        ...(selects
          ? [
              helper.display({
                id: "select",
                header: () => (
                  <Checkbox
                    aria-label={t("products:actions.selectAll")}
                    checked={allTicked}
                    onCheckedChange={(on) =>
                      setTicked(on === true ? new Set(rows.map((r) => r.id)) : new Set())
                    }
                  />
                ),
                cell: ({ row }) => (
                  // A tick never opens the product: the row's click and Enter stop here.
                  <span
                    className="inline-flex"
                    onClick={(e) => e.stopPropagation()}
                    onKeyDown={(e) => e.stopPropagation()}
                  >
                    <Checkbox
                      aria-label={t("products:actions.select", { name: localizedName(row.original, lang) })}
                      checked={ticked.has(row.original.id)}
                      onCheckedChange={(on) => setTicked((all) => toggled(all, row.original.id, on === true))}
                    />
                  </span>
                ),
              }),
            ]
          : []),
        helper.accessor((row) => localizedName(row, lang), {
          id: "name",
          header: () => t("products:columns.name"),
        }),
        helper.accessor("reference", {
          header: () => t("products:columns.reference"),
          cell: (i) => <bdi dir="ltr">{i.getValue()}</bdi>,
        }),
        helper.accessor("barcode", {
          header: () => t("products:columns.barcode"),
          cell: (i) => <bdi dir="ltr">{i.getValue()}</bdi>,
        }),
        helper.accessor("sellPrice", {
          header: () => t("products:columns.price"),
          cell: (i) => <Amount value={i.getValue()} />,
        }),
        helper.accessor("stock", {
          header: () => t("products:columns.stock"),
          cell: (i) => <bdi dir="ltr">{formatQty(i.getValue(), lang)}</bdi>,
        }),
        helper.display({
          id: "status",
          header: () => t("products:columns.status"),
          cell: ({ row }) =>
            row.original.archived ? (
              <Badge tone="neutral">{t("products:status.archived")}</Badge>
            ) : !row.original.availableAtTill ? (
              <Badge tone="warn">{t("products:status.notAtTill")}</Badge>
            ) : null,
        }),
      ]),
    [lang, t, selects, ticked, allTicked, rows],
  );
  const table = useTable({ features, columns, data: rows });
  const scroller = useRef<HTMLDivElement>(null);
  const tableRows = table.getRowModel().rows;
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual (spec §10.4)
  const virtualizer = useVirtualizer({
    count: tableRows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => 40,
    overscan: 10,
    initialRect: { width: 1000, height: 600 },
  });
  const items = virtualizer.getVirtualItems();
  const top = items[0]?.start ?? 0;
  const bottom = virtualizer.getTotalSize() - (items.at(-1)?.end ?? 0);
  const endAligned = new Set(["sellPrice", "stock"]);
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <h1 className="text-3xl font-bold">{t("products:title")}</h1>
        <div className="flex flex-wrap gap-2">
          {canExport ? <ExportMenu target={{ kind: "products" }} /> : null}
          <ImportLink entities={["products", "categories"]} />
          <Button asChild variant="outline">
            <Link to="/products/promotions">
              <BadgePercent aria-hidden />
              {t("products:actions.promotions")}
            </Link>
          </Button>
          <Button asChild variant="outline">
            <Link to="/products/labels">
              <Tag aria-hidden />
              {t("labels:title")}
            </Link>
          </Button>
          {canLabel && ticked.size > 0 ? (
            <Button variant="outline" onClick={() => setBulk("labels")}>
              <Printer aria-hidden />
              {t("labels:print.open")}
            </Button>
          ) : null}
          {canEdit && ticked.size > 0 ? (
            <Button variant="outline" onClick={() => setBulk("edit")}>
              <PencilLine aria-hidden />
              {t("products:actions.bulkEdit", { count: ticked.size })}
            </Button>
          ) : null}
          {canPrice ? (
            <Button variant="outline" onClick={() => setBulk("price")}>
              <TrendingUp aria-hidden />
              {t("products:actions.bulkPrice")}
            </Button>
          ) : null}
          {canEdit ? (
            <>
              {till.isTill ? null : (
                <Button variant="outline" onClick={() => setCategoriesOpen(true)}>
                  <Tags aria-hidden />
                  {t("products:categories.button")}
                </Button>
              )}
              <Button variant="primary" onClick={() => setOpen("new")}>
                <Plus aria-hidden />
                {t("products:dialog.new")}
              </Button>
            </>
          ) : null}
        </div>
      </div>
      {tabs}
      {till.isTill ? (
        <p role="note" className="rounded-control bg-info-soft p-3 text-info">
          {t(till.online ? "sync:lan.shell.hostOnlyOnline" : "sync:lan.shell.hostOnly")}
        </p>
      ) : null}
      <div className="flex flex-wrap items-end gap-3">
        <Field id="products-search" label={t("products:search")}>
          <ScanInput
            id="products-search"
            dir="auto"
            value={text}
            onChange={(e) => setText(e.target.value)}
            onScan={setText}
          />
        </Field>
        <Field id="products-category" label={t("products:fields.category")}>
          <NativeSelect id="products-category" value={category} onChange={(e) => setCategory(e.target.value)}>
            <option value="">{t("pos:categories.all")}</option>
            {categories.map((c) => (
              <option key={c.id} value={c.id}>
                {localizedName(c, lang)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id="products-archived" label={t("products:filters.show")}>
          <NativeSelect
            id="products-archived"
            value={archived}
            onChange={(e) => setArchived(e.target.value as ArchiveFilter)}
          >
            {ARCHIVE_FILTERS.map((f) => (
              <option key={f} value={f}>
                {t(`products:filters.${f}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
      </div>
      <div
        ref={scroller}
        className="max-h-[65dvh] overflow-y-auto rounded-card border border-line bg-surface"
      >
        <table className="w-full border-collapse">
          <thead className="sticky top-0 bg-surface text-sm text-ink-2">
            {table.getHeaderGroups().map((group) => (
              <tr key={group.id} className="h-10">
                {group.headers.map((header) => (
                  <th
                    key={header.id}
                    className={
                      endAligned.has(header.column.id)
                        ? "px-3 text-end font-semibold"
                        : "px-3 text-start font-semibold"
                    }
                  >
                    {header.isPlaceholder ? null : <table.FlexRender header={header} />}
                  </th>
                ))}
              </tr>
            ))}
          </thead>
          <tbody>
            {top > 0 ? <tr style={{ height: top }} /> : null}
            {items.map((item) => {
              const row = tableRows[item.index];
              if (!row) return null;
              return (
                <tr
                  key={row.id}
                  tabIndex={0}
                  onClick={() => setOpen(row.original.id)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") setOpen(row.original.id);
                  }}
                  className="h-10 cursor-pointer border-t border-line even:bg-surface-2 hover:bg-brand-soft"
                >
                  {row.getAllCells().map((cell) => (
                    <td key={cell.id} className={endAligned.has(cell.column.id) ? "px-3 text-end" : "px-3"}>
                      <table.FlexRender cell={cell} />
                    </td>
                  ))}
                </tr>
              );
            })}
            {bottom > 0 ? <tr style={{ height: bottom }} /> : null}
          </tbody>
        </table>
      </div>
      {query === "" && catalog.hasNextPage ? (
        <div>
          <Button
            variant="outline"
            disabled={catalog.isFetchingNextPage}
            onClick={() => void catalog.fetchNextPage()}
          >
            {t("common:actions.loadMore")}
          </Button>
        </div>
      ) : null}
      {open === "new" ? (
        <ProductDialog product={null} session={session} onClose={() => setOpen(null)} />
      ) : null}
      {open !== null && open !== "new" && selected.data ? (
        <ProductDialog
          key={selected.data.id}
          product={selected.data}
          session={session}
          onClose={() => setOpen(null)}
        />
      ) : null}
      {categoriesOpen ? <CategoriesDialog onClose={() => setCategoriesOpen(false)} /> : null}
      {bulk === "price" ? <BulkPriceDialog productIds={[...ticked]} onClose={() => setBulk(null)} /> : null}
      {bulk === "edit" ? <BulkEditDialog productIds={[...ticked]} onClose={() => setBulk(null)} /> : null}
      {bulk === "labels" ? (
        <PrintLabelsDialog
          source={{
            kind: "products",
            items: [...ticked].map((productId) => ({ productId, copies: 1, lotId: null })),
          }}
          onClose={() => setBulk(null)}
        />
      ) : null}
    </div>
  );
}
