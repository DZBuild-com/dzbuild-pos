import { zodResolver } from "@hookform/resolvers/zod";
import type { Resolver } from "react-hook-form";
import { z } from "zod";
import { formatAmountInput } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import type { LevelPrice, Product, ProductInput, ProductKind } from "@/lib/ipc/types";
import { formatQty, parseAmount, parseQty } from "@/lib/numeric";

/** Kinds this form edits; a variant and a kit keep their kind (a variant is made by the variants
 * matrix of its model, PRD-020). */
export const EDITABLE_KINDS = [
  "simple",
  "service",
  "variant_parent",
] as const satisfies readonly ProductKind[];
type EditableKind = (typeof EDITABLE_KINDS)[number];

function isEditableKind(kind: ProductKind): kind is EditableKind {
  return (EDITABLE_KINDS as readonly ProductKind[]).includes(kind);
}

const amount = z.string().refine((v) => parseAmount(v) !== null, "validation:amountFormat");
const optionalAmount = z
  .string()
  .refine((v) => v.trim() === "" || parseAmount(v) !== null, "validation:amountFormat");

export const productSchema = z.object({
  nameAr: z.string().max(120, "validation:tooLong"),
  nameFr: z.string().max(120, "validation:tooLong"),
  nameEn: z.string().max(120, "validation:tooLong"),
  reference: z.string().max(48, "validation:tooLong"),
  categoryId: z.string(),
  unitId: z.string().min(1, "validation:required"),
  taxRateId: z.string().min(1, "validation:required"),
  kind: z.enum(EDITABLE_KINDS),
  sellPrice: amount,
  costPrice: optionalAmount,
  minStock: z.string().refine((v) => v.trim() === "" || parseQty(v, 3) !== null, "validation:qtyFormat"),
  trackStock: z.boolean(),
  allowNegativeStock: z.boolean(),
  isWeighed: z.boolean(),
  availableAtTill: z.boolean(),
  plu: z.string().refine((v) => v === "" || (/^\d{1,5}$/.test(v) && Number(v) >= 1), "validation:pluFormat"),
  barcode: z.string().max(48, "validation:tooLong"),
  levelPrices: z.record(z.string(), optionalAmount),
});

export type ProductForm = z.infer<typeof productSchema>;

const schemaResolver = zodResolver(productSchema);

/** The schema's field checks plus "at least one name". A whole-object `refine` would be skipped
 * while another field fails, and the cashier must see every problem at once. */
export const productResolver: Resolver<ProductForm> = async (values, context, options) => {
  const result = await schemaResolver(values, context, options);
  if ([values.nameAr, values.nameFr, values.nameEn].some((n) => n.trim() !== "")) return result;
  return {
    values: {},
    errors: { ...result.errors, nameFr: { type: "validate", message: "validation:nameRequired" } },
  };
};

export function canEditKind(product: Product | null): boolean {
  return product === null || isEditableKind(product.kind);
}

export function emptyForm(unitId: string, taxRateId: string): ProductForm {
  return {
    nameAr: "",
    nameFr: "",
    nameEn: "",
    reference: "",
    categoryId: "",
    unitId,
    taxRateId,
    kind: "simple",
    sellPrice: "",
    costPrice: "",
    minStock: "",
    trackStock: true,
    allowNegativeStock: false,
    isWeighed: false,
    availableAtTill: true,
    plu: "",
    barcode: "",
    levelPrices: {},
  };
}

export function toForm(p: Product, lang: Lang): ProductForm {
  return {
    nameAr: p.nameAr,
    nameFr: p.nameFr,
    nameEn: p.nameEn,
    reference: p.reference ?? "",
    categoryId: p.categoryId ?? "",
    unitId: p.unitId,
    taxRateId: p.taxRateId,
    kind: isEditableKind(p.kind) ? p.kind : "simple",
    sellPrice: formatAmountInput(p.sellPrice, lang),
    costPrice: formatAmountInput(p.costPrice, lang),
    minStock: p.minStock === 0 ? "" : formatQty(p.minStock, lang),
    trackStock: p.trackStock,
    allowNegativeStock: p.allowNegativeStock,
    isWeighed: p.isWeighed,
    availableAtTill: p.availableAtTill,
    plu: p.plu === null ? "" : String(p.plu),
    barcode: p.barcodes[0]?.barcode ?? "",
    levelPrices: Object.fromEntries(
      p.prices.map((lp) => [lp.priceLevelId, formatAmountInput(lp.price, lang)]),
    ),
  };
}

/** The input M1a `products::create/update` takes. An existing product keeps its other barcodes,
 * its kind when it is a variant or a kit, its parent, image and components. An empty cost keeps
 * the stored cost (the backend also keeps it for users who cannot see costs). */
export function toInput(v: ProductForm, existing: Product | null): ProductInput {
  const prices: LevelPrice[] = Object.entries(v.levelPrices).flatMap(([priceLevelId, text]) => {
    const price = text.trim() === "" ? null : parseAmount(text);
    return price === null ? [] : [{ priceLevelId, price }];
  });
  const others = (existing?.barcodes ?? []).slice(1);
  const kind: ProductKind = existing && !canEditKind(existing) ? existing.kind : v.kind;
  return {
    reference: v.reference.trim() === "" ? null : v.reference.trim(),
    nameAr: v.nameAr.trim(),
    nameFr: v.nameFr.trim(),
    nameEn: v.nameEn.trim(),
    categoryId: v.categoryId === "" ? null : v.categoryId,
    unitId: v.unitId,
    taxRateId: v.taxRateId,
    kind,
    parentId: existing?.parentId ?? null,
    sellPrice: parseAmount(v.sellPrice) ?? 0,
    costPrice: v.costPrice.trim() === "" ? (existing?.costPrice ?? 0) : (parseAmount(v.costPrice) ?? 0),
    minStock: v.minStock.trim() === "" ? 0 : (parseQty(v.minStock, 3) ?? 0),
    trackStock: kind === "service" ? false : v.trackStock,
    allowNegativeStock: v.allowNegativeStock,
    isWeighed: v.isWeighed,
    availableAtTill: v.availableAtTill,
    plu: v.plu === "" ? null : Number(v.plu),
    image: existing?.image ?? null,
    barcodes:
      v.barcode.trim() === ""
        ? others
        : [{ barcode: v.barcode.trim(), packQty: 1_000, label: null }, ...others],
    prices,
    components: existing?.components ?? [],
  };
}
