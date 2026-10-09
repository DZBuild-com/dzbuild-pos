import { FileSpreadsheet } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { NativeSelect } from "@/components/ui/native-select";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { type Lang, LANGS, isLang } from "@/lib/i18n";
import { useExportPlu, useScaleItems, useSetScaleItem } from "@/lib/ipc/labels";
import { useTillLink } from "@/lib/ipc/lan";
import type { ScaleItem, ScaleItemInput, SheetFormat } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

type ScaleField = keyof ScaleItemInput;

/** A typed scale value: empty is "none" for the hotkey and the shelf life, 0 g for the tare. */
function parsed(field: ScaleField, text: string): number | null {
  const value = text.trim();
  if (value === "") return field === "tareG" ? 0 : null;
  return Number(value);
}

function shown(value: number | null): string {
  return value === null ? "" : String(value);
}

/** One scale setting of an item, typed in the table and saved when the field is left (or Enter). */
function ScaleCell({
  item,
  field,
  label,
  editable,
  onSave,
}: {
  item: ScaleItem;
  field: ScaleField;
  label: string;
  editable: boolean;
  onSave: (item: ScaleItem, field: ScaleField, value: number | null) => void;
}) {
  const initial = shown(item[field]);
  const [text, setText] = useState(initial);
  const commit = () => {
    if (text === initial) return;
    onSave(item, field, parsed(field, text));
  };
  return (
    <NumericInput
      kind="integer"
      aria-label={label}
      disabled={!editable}
      value={text}
      onValueChange={setText}
      onBlur={commit}
      onKeyDown={(event) => {
        // arrows, Home and End stay in the field instead of moving the row focus
        event.stopPropagation();
        if (event.key === "Enter") {
          event.preventDefault();
          event.currentTarget.blur();
        }
      }}
      className="h-9 text-end"
    />
  );
}

const col = columnHelper<ScaleItem>();

/**
 * PRD-074: the goods of the label scales, by PLU. The scale key, the tare and the shelf life are
 * typed in the table and saved field by field; the PLU list goes to the scale maker's PC tool as
 * an Excel or CSV file written by Rust, in the language chosen here. `editable`: the user holds
 * `products.edit` (Rust checks it again on every save). On a till the scale items are the main
 * PC's: read-only, the PLU file is still exported.
 */
export function ScaleItemsTab({ editable: allowed = true }: { editable?: boolean }) {
  const { t } = useTranslation();
  const till = useTillLink();
  const editable = allowed && !till.isTill;
  const lang = useLang();
  const errorText = useErrorText();
  const items = useScaleItems();
  const setItem = useSetScaleItem();
  const exporter = useExportPlu();
  const [fileLang, setFileLang] = useState<Lang>(lang);
  const rows = useMemo(() => items.data ?? [], [items.data]);
  const { mutate: saveItem } = setItem;

  const save = useCallback(
    (item: ScaleItem, field: ScaleField, value: number | null) => {
      const input: ScaleItemInput = {
        hotkey: item.hotkey,
        tareG: item.tareG,
        shelfLifeDays: item.shelfLifeDays,
      };
      saveItem(
        { productId: item.productId, input: { ...input, [field]: value } },
        { onError: (error) => toast.error(errorText(error)) },
      );
    },
    [saveItem, errorText],
  );

  const columns = useMemo(() => {
    const cell = (field: ScaleField, labelKey: string) =>
      col.display({
        id: field,
        header: () => t(`labels:scale.${labelKey}`),
        meta: { align: "end" },
        cell: ({ row }) => (
          <ScaleCell
            // a saved value from the backend replaces what the field shows
            key={`${row.original.productId}:${shown(row.original[field])}`}
            item={row.original}
            field={field}
            label={t(`labels:scale.${labelKey}For`, { name: localizedName(row.original, lang) })}
            editable={editable}
            onSave={save}
          />
        ),
      });
    return col.columns([
      col.accessor("plu", {
        header: () => t("labels:scale.plu"),
        cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
      }),
      col.accessor((r) => localizedName(r, lang), {
        id: "name",
        header: () => t("labels:scale.product"),
      }),
      cell("hotkey", "hotkey"),
      cell("tareG", "tare"),
      cell("shelfLifeDays", "shelfLife"),
      col.accessor("price", {
        header: () => t("labels:scale.price"),
        meta: { align: "end" },
        cell: (c) => <Amount value={c.getValue()} />,
      }),
      col.accessor((r) => r.barcodePrefix ?? "", {
        id: "barcode",
        header: () => t("labels:scale.barcode"),
        cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
      }),
    ]);
  }, [t, lang, editable, save]);

  const exportAs = (format: SheetFormat) =>
    exporter.mutate(
      { format, lang: fileLang },
      {
        // `null`: the save dialog was cancelled
        onSuccess: (file) => {
          if (file) toast.success(t("labels:scale.exported", { file }));
        },
        onError: (error) => toast.error(errorText(error)),
      },
    );

  return (
    <div className="flex flex-col gap-4">
      <HostOnlyNote />
      <div className="flex flex-wrap items-center justify-end gap-2">
        <NativeSelect
          aria-label={t("labels:scale.fileLang")}
          className="w-auto"
          value={fileLang}
          onChange={(event) => {
            if (isLang(event.target.value)) setFileLang(event.target.value);
          }}
        >
          {LANGS.map((l) => (
            // each language in its own script, whatever the UI language
            <option key={l} value={l} lang={l}>
              {t(`common:languages.${l}`)}
            </option>
          ))}
        </NativeSelect>
        <Button
          variant="secondary"
          disabled={rows.length === 0 || exporter.isPending}
          onClick={() => exportAs("xlsx")}
        >
          <FileSpreadsheet aria-hidden />
          {t("labels:scale.export")}
        </Button>
        <Button
          variant="secondary"
          disabled={rows.length === 0 || exporter.isPending}
          onClick={() => exportAs("csv")}
        >
          {t("labels:scale.exportCsv")}
        </Button>
      </div>
      {items.error ? (
        <p role="alert" className="text-danger">
          {errorText(items.error)}
        </p>
      ) : null}
      <DataTable
        ariaLabel={t("labels:scale.list")}
        data={rows}
        columns={columns}
        getRowId={(r) => r.productId}
        empty={items.isPending ? t("common:status.loading") : t("labels:scale.empty")}
      />
    </div>
  );
}
