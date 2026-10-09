import { Grid3x3, X } from "lucide-react";
import { type KeyboardEvent, Suspense, useCallback, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { ScanInput } from "@/components/scan-input";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { VariantGridDialog } from "@/features/packs/pack-screens";
import type { EnteredLine } from "@/features/products/VariantGrid";
import { useUnits } from "@/lib/ipc/catalog";
import { useSerialScans } from "@/lib/ipc/devices";
import { useScannerOptions } from "@/lib/ipc/hardware";
import { useModule } from "@/lib/ipc/pack-state";
import { useExtrasOf } from "@/lib/ipc/product-extras";
import { useFindByCode, useProductSearch } from "@/lib/ipc/products";
import { useSupplierSearch } from "@/lib/ipc/suppliers";
import type { PurchaseLineInput, ScanHit } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { normalizeDigits, parseAmount, parseQty } from "@/lib/numeric";
import { OWN_SCAN, useScanner } from "@/lib/scanner";
import { beep } from "@/lib/sounds";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** A purchase line as typed: text fields, turned into a `PurchaseLineInput` on save. */
export interface LineDraft {
  key: string;
  productId: string;
  name: string;
  unitCode: string;
  /** Units, or packs when `pack` is set. */
  qty: string;
  /** Units in one pack (PRD-031: a carton of 12); empty when the line counts units. */
  pack: string;
  /** HT cost per unit, or per pack when `pack` is set. */
  cost: string;
  /** The order line a receipt line comes from. */
  originLineId: string | null;
  lotNumber: string;
  expiryDate: string;
  serials: string[];
}

export type LineField = "qty" | "pack" | "cost" | "lotNumber" | "serials";
/** What is wrong with a line, as translation keys or texts, by field. */
export type LineErrors = Partial<Record<LineField, string>>;

let lastKey = 0;
/** A key for a line added in the UI (order lines keep their line id). */
export function newLineKey(): string {
  lastKey += 1;
  return `line-${lastKey}`;
}

/** A product picked by search or scan; a pack barcode (a carton) brings its pack size. */
export interface Picked {
  productId: string;
  name: string;
  unitCode: string;
  /** Units in the scanned pack, in milli-units; `null` for a unit barcode or a search. */
  packQty: number | null;
}

export function emptyLine(p: Picked, packText: string): LineDraft {
  return {
    key: newLineKey(),
    productId: p.productId,
    name: p.name,
    unitCode: p.unitCode,
    qty: "1",
    pack: packText,
    cost: "",
    originLineId: null,
    lotNumber: "",
    expiryDate: "",
    serials: [],
  };
}

/** "N° de lot — Lait UHT 1 L": the accessible name of a field of one line. */
export function useLineLabel(): (fieldKey: string, name: string) => string {
  const { t } = useTranslation();
  return (fieldKey, name) => t("purchases:lineField", { field: t(fieldKey), name });
}

/** Keys typed in a field inside a table row stay in the field (the row would move the focus on
 * arrows, Home and End). */
export function keepKeysInField(event: KeyboardEvent<HTMLElement>): void {
  event.stopPropagation();
}

/** Whether each product is received by lot or by serial number (Task 19's product extras); an
 * unreadable product counts as neither, and the backend still refuses a missing lot or serial. */
export function useTracking(productIds: readonly string[]) {
  const ids = [...new Set(productIds)];
  const results = useExtrasOf(ids);
  const known = new Map(ids.map((id, i) => [id, results[i]]));
  return {
    ready: results.every((r) => !r.isPending),
    of: (productId: string) => {
      const result = known.get(productId);
      if (!result || result.isPending) return null;
      return { lots: result.data?.trackLots ?? false, serials: result.data?.trackSerials ?? false };
    },
  };
}

/** The decimals a quantity of this unit may have (0 for pieces, 3 for kg); 3 until the units are
 * read, and the backend refuses what the unit does not allow. */
export function useUnitDecimals(): (unitCode: string) => number {
  const units = useUnits().data;
  return useCallback((unitCode: string) => units?.find((u) => u.code === unitCode)?.decimals ?? 3, [units]);
}

const ZERO = /^0*([.,]0*)?$/;

/** Nothing typed, or zero: a receipt skips the line (partial deliveries). */
export function isNothing(qty: string): boolean {
  return ZERO.test(normalizeDigits(qty).replace(/\s/g, ""));
}

/**
 * The line as the backend takes it, or what is wrong with it. `tracking` (receipts only) asks for
 * the lot of a lot-tracked product and one serial number per piece of a serial-tracked one.
 */
export function toLineInput(
  line: LineDraft,
  decimals: number,
  tracking: { lots: boolean; serials: boolean } | null = null,
): { input: PurchaseLineInput; errors: null } | { input: null; errors: LineErrors } {
  const errors: LineErrors = {};
  const pack = line.pack.trim() === "" ? null : parseQty(line.pack, decimals);
  if (line.pack.trim() !== "" && pack === null) errors.pack = "validation:qtyFormat";
  // A number of packs is whole; units take the unit's decimals.
  const qty = parseQty(line.qty, line.pack.trim() === "" ? decimals : 0);
  if (qty === null) errors.qty = "validation:qtyFormat";
  const cost = parseAmount(line.cost);
  if (cost === null) errors.cost = "validation:amountFormat";
  if (tracking?.lots && line.lotNumber.trim() === "") errors.lotNumber = "validation:required";
  // One serial number per piece: a pack line holds packs × units per pack pieces.
  const pieces = qty !== null && pack !== null ? Math.round((qty * pack) / 1_000) : qty;
  if (tracking?.serials && pieces !== null && line.serials.length * 1_000 !== pieces) {
    errors.serials = "purchases:receive.serialsCount";
  }
  if (qty === null || cost === null || Object.keys(errors).length > 0) return { input: null, errors };
  return {
    input: {
      productId: line.productId,
      qty,
      unitCost: cost,
      packQty: pack,
      taxRateId: null,
      originLineId: line.originLineId,
      lotNumber: tracking?.lots ? line.lotNumber.trim() : null,
      expiryDate: tracking?.lots && line.expiryDate !== "" ? line.expiryDate : null,
      serials: tracking?.serials ? line.serials : [],
      note: null,
    },
    errors: null,
  };
}

/** `lines.2.lotNumber` → 2: the line a backend refusal names. */
export function refusedLine(field: string | null | undefined): number | null {
  const match = /^lines\.(\d+)\./.exec(field ?? "");
  return match ? Number(match[1]) : null;
}

/**
 * PUR-010: adds a product by name, reference or barcode. A scan in the field (or a code typed and
 * Enter) looks the barcode up, a carton's barcode bringing its pack size; Enter on a name takes the
 * first result. With `scanAnywhere` a scan is read wherever the focus is on the screen (a quantity
 * or a cost being typed: its burst guard drops the scan's keys), with a beep, so "scan, type the
 * cost, scan the next carton" loses no product; a field marked `OWN_SCAN` (serial numbers) keeps
 * its own scans.
 */
export function ProductPicker({
  onPick,
  autoFocus,
  scanAnywhere = false,
}: {
  onPick: (p: Picked) => void;
  autoFocus?: boolean;
  scanAnywhere?: boolean;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
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

  const fromScan = (hit: ScanHit): Picked => ({
    productId: hit.product.id,
    name: localizedName(hit.product, lang),
    unitCode: hit.product.unitCode,
    packQty: hit.packQty !== 1_000 ? hit.packQty : null,
  });
  // One promise per code: scans in a quick row each add their product (a mutation's own
  // callbacks only follow the latest call).
  const lookup = (value: string, sound = false) => {
    const code = normalizeDigits(value.trim());
    setText("");
    if (code === "") return;
    find
      .mutateAsync(code)
      .then((hit) => {
        if (hit) {
          if (sound) beep("ok");
          setProblem(null);
          onPick(fromScan(hit));
        } else {
          if (sound) beep("error");
          setProblem(t("purchases:lines.notFound", { code }));
        }
      })
      .catch((error: unknown) => {
        if (sound) beep("error");
        setProblem(errorText(error));
      });
  };
  const scanner = useScannerOptions();
  useScanner((code) => lookup(code, true), { ...scanner, enabled: scanAnywhere });
  // a scanner in USB-COM mode reads into the same lookup (HW-020)
  useSerialScans((code) => lookup(code, true), scanAnywhere);
  const pick = (p: Picked) => {
    setText("");
    setProblem(null);
    onPick(p);
  };

  return (
    <div className="flex flex-col gap-2">
      <Field
        id={id}
        label={t("purchases:lines.search")}
        hint={t("purchases:lines.searchHint")}
        error={problem ?? undefined}
      >
        <ScanInput
          id={id}
          type="search"
          dir="auto"
          autoFocus={autoFocus}
          value={text}
          onChange={(event) => setText(event.target.value)}
          onScan={(code) => lookup(code)}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            const first = shown[0];
            if (/^\d+$/.test(normalizeDigits(text.trim()))) lookup(text);
            else if (first) {
              pick({
                productId: first.id,
                name: localizedName(first, lang),
                unitCode: first.unitCode,
                packQty: null,
              });
            }
          }}
        />
      </Field>
      {shown.length > 0 ? (
        <ul aria-label={t("purchases:lines.results")} className="flex flex-wrap gap-2">
          {shown.map((hit) => (
            <li key={hit.id}>
              <Button
                variant="outline"
                onClick={() =>
                  pick({
                    productId: hit.id,
                    name: localizedName(hit, lang),
                    unitCode: hit.unitCode,
                    packQty: null,
                  })
                }
              >
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

/** VRT-060 "Saisir par grille" (variant grid module on): a variant model's quantities typed per
 * size and colour come back as one line per filled cell (a goods receipt, a count). */
export function GridEntryButton({ onAdd }: { onAdd: (lines: EnteredLine[]) => void }) {
  const { t } = useTranslation();
  const on = useModule("variant_grid");
  const [open, setOpen] = useState(false);
  if (!on) return null;
  return (
    <>
      <Button variant="outline" className="self-start" onClick={() => setOpen(true)}>
        <Grid3x3 aria-hidden />
        {t("products:variantGrid.enter")}
      </Button>
      {open ? (
        <Suspense fallback={null}>
          <VariantGridDialog
            onAdd={(lines) => {
              setOpen(false);
              onAdd(lines);
            }}
            onClose={() => setOpen(false)}
          />
        </Suspense>
      ) : null}
    </>
  );
}

/** Serial numbers of a line (IMEI for phones), scanned or typed one by one. */
export function SerialList({
  id,
  label,
  serials,
  onChange,
  error,
}: {
  id: string;
  label: string;
  serials: string[];
  onChange: (serials: string[]) => void;
  error?: string;
}) {
  const { t } = useTranslation();
  const [text, setText] = useState("");
  const add = (value: string) => {
    const serial = normalizeDigits(value).trim();
    setText("");
    if (serial !== "" && !serials.includes(serial)) onChange([...serials, serial]);
  };
  return (
    <Field id={id} label={label} hint={t("purchases:receive.serialsHint")} error={error}>
      <ScanInput
        id={id}
        {...OWN_SCAN}
        dir="ltr"
        value={text}
        maxLength={64}
        aria-invalid={Boolean(error)}
        onChange={(event) => setText(event.target.value)}
        onScan={add}
        onKeyDown={(event) => {
          if (event.key !== "Enter") return;
          event.preventDefault();
          add(text);
        }}
      />
      {serials.length > 0 ? (
        <ul className="flex flex-wrap gap-2">
          {serials.map((serial) => (
            <li
              key={serial}
              className="inline-flex min-h-11 items-center gap-1 rounded-full bg-surface-2 ps-3 font-semibold"
            >
              <bdi dir="ltr">{serial}</bdi>
              <button
                type="button"
                aria-label={t("purchases:receive.serialRemove", { serial })}
                onClick={() => onChange(serials.filter((s) => s !== serial))}
                className="grid size-11 place-items-center rounded-full text-ink-2 hover:bg-surface"
              >
                <X aria-hidden className="size-4" />
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </Field>
  );
}

/** The active suppliers, by name, for a document's supplier. */
export function SupplierSelect({
  id,
  value,
  onChange,
  allLabel,
  error,
}: {
  id: string;
  value: string;
  onChange: (supplierId: string) => void;
  /** The empty choice ("Tous les fournisseurs" in a filter, "Choisissez…" in a form). */
  allLabel: string;
  error?: string;
}) {
  const { t } = useTranslation();
  const suppliers = useSupplierSearch({ limit: 5_000 });
  return (
    <Field id={id} label={t("purchases:columns.supplier")} error={error}>
      <NativeSelect
        id={id}
        value={value}
        aria-invalid={Boolean(error)}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="">{allLabel}</option>
        {(suppliers.data ?? []).map((s) => (
          <option key={s.id} value={s.id}>
            {s.name}
          </option>
        ))}
      </NativeSelect>
    </Field>
  );
}
