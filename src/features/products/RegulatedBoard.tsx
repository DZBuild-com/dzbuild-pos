import { Link2, Link2Off, PackagePlus } from "lucide-react";
import { useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { ProductPicker } from "@/features/purchases/purchase-lines";
import { PolicySetting } from "@/features/packs/setting-cards";
import { useCan } from "@/features/stock/stock-parts";
import { useDatapack } from "@/lib/ipc/app";
import { useRegulatedBoard, useSeedRegulated, useSetRegulated } from "@/lib/ipc/packs";
import type { RegulatedItem, RegulatedProduct } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatDZD } from "@/lib/format";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

const col = columnHelper<RegulatedProduct>();

/**
 * PRC-012: the products held to a legal price ceiling (the data pack's subsidised staples), each
 * with its ceiling and its current consumer price, in red when above. A product is linked to its
 * ceiling here ("Lier à un produit réglementé"), and "Créer les produits de base" adds the pack's
 * staples missing from the catalogue. Prices are changed in the product form, never here.
 */
export function RegulatedBoard() {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const errorText = useErrorText();
  const board = useRegulatedBoard();
  const items = useDatapack().data?.regulated.items;
  const setRegulated = useSetRegulated();
  const seed = useSeedRegulated();
  const [search, setSearch] = useState("");
  const [linking, setLinking] = useState(false);
  const manages = can("products.price");
  const itemName = useMemo(() => {
    const byCode = new Map((items ?? []).map((i) => [i.code, i]));
    return (code: string) => {
      const item = byCode.get(code);
      return item ? localizedName(item, lang) : code;
    };
  }, [items, lang]);

  const text = search.trim().toLowerCase();
  const rows = useMemo(
    () =>
      (board.data ?? []).filter(
        (r) =>
          text === "" ||
          [r.nameFr, r.nameAr, r.nameEn, itemName(r.code)].some((v) => v.toLowerCase().includes(text)),
      ),
    [board.data, text, itemName],
  );
  const columns = useMemo(
    () =>
      col.columns([
        col.accessor((r) => localizedName(r, lang), {
          id: "product",
          header: () => t("products:regulated.product"),
        }),
        col.accessor((r) => itemName(r.code), { id: "code", header: () => t("products:regulated.code") }),
        col.accessor("ceiling", {
          header: () => t("products:regulated.ceiling"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("priceTtc", {
          header: () => t("products:regulated.price"),
          meta: { align: "end" },
          cell: (c) => (
            <Amount value={c.getValue()} className={cn(c.row.original.above && "font-bold text-danger")} />
          ),
        }),
        col.accessor("above", {
          header: () => t("products:regulated.status"),
          cell: (c) =>
            c.getValue() ? (
              <Badge tone="danger">{t("products:regulated.above")}</Badge>
            ) : (
              <Badge tone="ok">{t("products:regulated.within")}</Badge>
            ),
        }),
        ...(manages
          ? [
              col.display({
                id: "actions",
                header: () => t("products:regulated.unlink"),
                cell: (c) => (
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={t("products:regulated.unlinkOf", {
                      name: localizedName(c.row.original, lang),
                    })}
                    onClick={(event) => {
                      event.stopPropagation();
                      setRegulated.mutate(
                        { productId: c.row.original.productId, code: null },
                        { onError: (error) => toast.error(errorText(error)) },
                      );
                    }}
                  >
                    <Link2Off aria-hidden />
                  </Button>
                ),
              }),
            ]
          : []),
      ]),
    [t, lang, itemName, manages, setRegulated, errorText],
  );

  const seedNow = () =>
    seed.mutate(undefined, {
      onSuccess: (n) => toast.success(t("products:regulated.seeded", { n })),
      onError: (error) => toast.error(errorText(error)),
    });
  return (
    <section className="flex flex-col gap-4" aria-labelledby="regulated-title">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex flex-col gap-1">
          <h2 id="regulated-title" className="text-2xl font-bold">
            {t("products:regulated.title")}
          </h2>
          <p className="text-ink-2">{t("products:regulated.intro")}</p>
        </div>
        {manages ? (
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" disabled={seed.isPending} onClick={seedNow}>
              <PackagePlus aria-hidden />
              {t("products:regulated.seed")}
            </Button>
            <Button variant="primary" onClick={() => setLinking(true)}>
              <Link2 aria-hidden />
              {t("products:regulated.link")}
            </Button>
          </div>
        ) : null}
      </div>
      {can("settings.company") ? (
        // SAL-030 (final review): a ticket above a ceiling warns, or is refused
        <PolicySetting
          settingKey="sales.regulated_policy"
          title={t("packs:settings.regulated.title")}
          help={t("packs:settings.regulated.help")}
          warn={t("packs:settings.regulated.warn")}
          block={t("packs:settings.regulated.block")}
        />
      ) : null}
      <TableToolbar search={search} onSearch={setSearch} exportTarget={null} />
      {board.error ? (
        <p role="alert" className="text-danger">
          {errorText(board.error)}
        </p>
      ) : null}
      <DataTable
        ariaLabel={t("products:regulated.title")}
        data={rows}
        columns={columns}
        getRowId={(r) => r.productId}
        empty={board.isPending ? t("common:status.loading") : t("products:regulated.empty")}
      />
      {linking ? <LinkDialog items={items ?? []} onClose={() => setLinking(false)} /> : null}
    </section>
  );
}

/** A product of the catalogue linked to one of the data pack's ceilings. */
function LinkDialog({ items, onClose }: { items: RegulatedItem[]; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const id = useId();
  const setRegulated = useSetRegulated();
  const [product, setProduct] = useState<{ productId: string; name: string } | null>(null);
  const [code, setCode] = useState("");
  const submit = () => {
    if (!product || code === "") return;
    setRegulated.mutate(
      { productId: product.productId, code },
      {
        onSuccess: () => {
          toast.success(t("products:regulated.linked"));
          onClose();
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("products:regulated.linkTitle")}
      footer={
        <Button
          variant="primary"
          disabled={!product || code === "" || setRegulated.isPending}
          onClick={submit}
        >
          {t("products:regulated.link")}
        </Button>
      }
    >
      <div className="flex flex-col gap-4">
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 font-semibold">{t("products:regulated.product")}</legend>
          {product ? <p className="font-semibold">{product.name}</p> : null}
          <ProductPicker onPick={(p) => setProduct({ productId: p.productId, name: p.name })} />
        </fieldset>
        <Field id={`${id}-code`} label={t("products:regulated.code")}>
          <NativeSelect id={`${id}-code`} value={code} onChange={(event) => setCode(event.target.value)}>
            <option value="">{t("products:regulated.choose")}</option>
            {items.map((item) => (
              <option key={item.code} value={item.code}>
                {t("products:regulated.ceilingOf", {
                  name: localizedName(item, lang),
                  ceiling: formatDZD(item.ceiling, lang),
                })}
              </option>
            ))}
          </NativeSelect>
        </Field>
        {setRegulated.error ? (
          <p role="alert" className="text-danger">
            {errorText(setRegulated.error)}
          </p>
        ) : null}
      </div>
    </Dialog>
  );
}
