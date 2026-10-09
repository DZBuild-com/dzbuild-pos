import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { type Lang } from "@/lib/i18n";
import { formatDate, formatDZD } from "@/lib/format";
import { useAuditHistory } from "@/lib/ipc/audit";
import { useSession } from "@/lib/ipc/auth";
import { useCategories, usePriceLevels } from "@/lib/ipc/catalog";
import { localizedName } from "@/lib/names";
import { formatQty, formatRate } from "@/lib/numeric";
import { useLang } from "@/lib/use-language";

/** Product snapshot fields never listed: bookkeeping, or shown through another field. */
const HIDDEN = new Set(["id", "updatedAt", "unitDecimals", "unitId", "taxRateId"]);
const MONEY = new Set(["sellPrice", "costPrice"]);
const QTY = new Set(["minStock"]);
/** Product fields and their label under `products:fields`. */
const LABELS: Record<string, string> = {
  nameFr: "nameFr",
  nameAr: "nameAr",
  nameEn: "nameEn",
  reference: "reference",
  categoryId: "category",
  unitCode: "unit",
  taxRateBp: "taxRate",
  kind: "kind",
  parentId: "parent",
  sellPrice: "sellPrice",
  costPrice: "costPrice",
  minStock: "minStock",
  trackStock: "trackStock",
  allowNegativeStock: "allowNegativeStock",
  isWeighed: "isWeighed",
  availableAtTill: "availableAtTill",
  plu: "plu",
  image: "image",
  archived: "archived",
  barcodes: "barcodes",
  prices: "prices",
  components: "components",
};

/** A payload value as text; a field absent on one side (added or removed) shows empty. */
function raw(value: unknown): string {
  return typeof value === "string" ? value : (JSON.stringify(value) ?? "");
}

interface Names {
  level: (id: string) => string;
  category: (id: string) => string;
}

/** A product field's value as a person reads it: amounts in DZD, quantities, yes/no, names. */
function shown(field: string, value: unknown, lang: Lang, t: TFunction, names: Names): string {
  if (value === null || value === undefined) return "";
  if (MONEY.has(field) && typeof value === "number") return formatDZD(value, lang);
  if (QTY.has(field) && typeof value === "number") return formatQty(value, lang);
  if (field === "taxRateBp" && typeof value === "number") return formatRate(value, lang);
  if (typeof value === "boolean") return t(value ? "common:answers.yes" : "common:answers.no");
  if (field === "kind" && typeof value === "string") return t(`products:kinds.${value}`);
  if (field === "categoryId" && typeof value === "string") return names.category(value);
  if (field === "barcodes" && Array.isArray(value)) {
    return value.map((b) => (b as { barcode?: string }).barcode ?? "").join(", ");
  }
  if (field === "prices" && Array.isArray(value)) {
    return value
      .map((p) => {
        const { priceLevelId, price } = p as { priceLevelId: string; price: number };
        return `${names.level(priceLevelId)} ${formatDZD(price, lang)}`;
      })
      .join(", ");
  }
  return raw(value);
}

/** AUD-002: every change of one record — who, when, and each field that changed, before → after,
 * with the fields named and the amounts in DZD. The purchase price is only for `reports.profit`
 * (the backend leaves it out for anyone else). */
export function EntityHistoryDialog({
  entityType,
  entityId,
  title,
  onClose,
}: {
  entityType: string;
  entityId: string;
  title: string;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = useLang();
  const history = useAuditHistory({ entityType, entityId, actionPrefix: null });
  const session = useSession().data;
  const levels = usePriceLevels().data ?? [];
  const categories = useCategories(true).data ?? [];
  const seesCosts = session?.permissions.includes("reports.profit") ?? false;
  const product = entityType === "product";
  const names: Names = {
    level: (id) => {
      const level = levels.find((l) => l.id === id);
      return level ? localizedName(level, lang) : id;
    },
    category: (id) => {
      const category = categories.find((c) => c.id === id);
      return category ? localizedName(category, lang) : id;
    },
  };
  const rows = history.data?.pages.flatMap((p) => p.rows) ?? [];
  const label = (action: string) =>
    i18n.exists(`settings:audit.actions.${action}`) ? t(`settings:audit.actions.${action}`) : action;
  const fieldLabel = (field: string) =>
    product && LABELS[field] ? t(`products:fields.${LABELS[field]}`) : field;
  const text = (field: string, value: unknown) =>
    product ? shown(field, value, lang, t, names) : raw(value);
  const listed = (field: string) => !product || (!HIDDEN.has(field) && (seesCosts || field !== "costPrice"));
  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("settings:history.title", { name: title })}
    >
      {rows.length === 0 && !history.isPending ? (
        <p className="text-ink-2">{t("settings:history.empty")}</p>
      ) : null}
      <ol className="flex flex-col gap-3">
        {rows.map((row) => {
          const payload = (row.payload ?? {}) as {
            before?: unknown;
            after?: unknown;
            priceLevelId?: string;
          };
          // A level price change carries the two prices themselves, not two snapshots.
          const scalar =
            payload.before !== undefined &&
            (typeof payload.before !== "object" || payload.before === null) &&
            typeof payload.priceLevelId === "string";
          const before = (scalar ? {} : (payload.before ?? {})) as Record<string, unknown>;
          const after = (scalar ? {} : (payload.after ?? {})) as Record<string, unknown>;
          const changed = Object.keys({ ...before, ...after }).filter(
            (k) => listed(k) && raw(before[k]) !== raw(after[k]),
          );
          return (
            <li key={row.seq} className="rounded-card border border-line p-3">
              <p className="font-semibold">
                {label(row.action)} · {row.userName ?? t("settings:audit.system")} ·{" "}
                <bdi dir="ltr">{formatDate(row.at, lang, { withTime: true })}</bdi>
              </p>
              {scalar && typeof payload.priceLevelId === "string" ? (
                <table className="mt-2 w-full text-sm">
                  <tbody>
                    <tr className="border-t border-line">
                      <td className="py-1 pe-3">
                        {t("products:fields.levelPrice", { level: names.level(payload.priceLevelId) })}
                      </td>
                      <td className="py-1 text-start">
                        <bdi dir="ltr">{text("sellPrice", payload.before)}</bdi>
                        {" → "}
                        <bdi dir="ltr">{text("sellPrice", payload.after)}</bdi>
                      </td>
                    </tr>
                  </tbody>
                </table>
              ) : payload.before !== undefined && changed.length > 0 ? (
                <table className="mt-2 w-full text-sm">
                  <tbody>
                    {changed.map((field) => (
                      <tr key={field} className="border-t border-line">
                        <td className={product && LABELS[field] ? "py-1 pe-3" : "py-1 pe-3 font-mono"}>
                          {fieldLabel(field)}
                        </td>
                        <td className="py-1 text-start">
                          <bdi dir="auto">{text(field, before[field])}</bdi>
                          {" → "}
                          <bdi dir="auto">{text(field, after[field])}</bdi>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              ) : null}
            </li>
          );
        })}
      </ol>
      {history.hasNextPage ? (
        <Button variant="outline" onClick={() => void history.fetchNextPage()}>
          {t("common:actions.loadMore")}
        </Button>
      ) : null}
    </Dialog>
  );
}
