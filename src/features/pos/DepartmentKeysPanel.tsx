import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useDepartments } from "@/lib/ipc/packs";
import type { DepartmentKey } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { OpenPriceDialog } from "./OpenPriceDialog";
import { useTillKit } from "./till-kit";

/**
 * SAL-019, VRT-001 (Ctrl+5): the department keys in place of the product grid. A key opens the
 * keypad for the price typed at the till (and a quantity), then adds an open-price line of the
 * key's product; the backend prices and taxes it like any line.
 */
export function DepartmentKeysPanel() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const { useCart } = useTillKit();
  const keys = useDepartments(false);
  const [picked, setPicked] = useState<DepartmentKey | null>(null);

  const list = keys.data ?? [];
  return (
    <section aria-label={t("packs:till.departments.title")} className="flex min-h-0 flex-1 flex-col gap-3">
      {keys.error ? (
        <p role="alert" className="text-danger">
          {errorText(keys.error)}
        </p>
      ) : null}
      {keys.isSuccess && list.length === 0 ? (
        <p className="rounded-control bg-surface p-4 text-ink-2">{t("packs:till.departments.empty")}</p>
      ) : null}
      <div className="grid grid-cols-[repeat(auto-fill,minmax(8rem,1fr))] gap-3 overflow-y-auto">
        {list.map((key) => (
          <button
            key={key.id}
            type="button"
            onClick={() => setPicked(key)}
            className="flex min-h-28 flex-col items-center justify-center gap-2 rounded-card border border-line bg-surface p-3 text-lg font-semibold hover:bg-surface-2 focus-visible:outline-3 focus-visible:outline-focus"
          >
            <span
              aria-hidden
              className="size-6 rounded-full border border-line"
              style={{ backgroundColor: key.color ?? "var(--surface-2)" }}
            />
            {localizedName(key, lang)}
          </button>
        ))}
      </div>
      {picked ? (
        <OpenPriceDialog
          title={localizedName(picked, lang)}
          unitDecimals={3}
          onAdd={(unitPrice, qty) =>
            useCart.getState().addLine(
              {
                productId: picked.productId,
                description: null,
                qty,
                unitPrice,
                discount: { kind: "none" },
                taxRateId: null,
                note: null,
                expectedUnitPrice: null,
              },
              { name: localizedName(picked, lang), unitCode: "pc", unitDecimals: 3, price: unitPrice },
            )
          }
          onClose={() => setPicked(null)}
        />
      ) : null}
    </section>
  );
}
