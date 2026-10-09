import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useProductSearch } from "@/lib/ipc/products";
import type { ProductHit } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useDebounced } from "@/lib/use-debounced";
import { useLang } from "@/lib/use-language";

/** A product found by its name, reference or barcode (the restaurant's editors: an option's
 * product, a product's options, the daily menu). */
export function ProductPicker({
  id,
  label,
  onPick,
}: {
  id: string;
  label: string;
  onPick: (hit: ProductHit) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const [text, setText] = useState("");
  const query = useDebounced(text.trim(), 150);
  const results = useProductSearch({ text: query, limit: 8, categoryId: null, includeArchived: false });
  const hits = query === "" ? [] : (results.data ?? []);
  return (
    <div className="flex flex-col gap-2">
      <Field id={id} label={label} hint={t("restaurant:editor.searchHint")}>
        <Input id={id} value={text} onChange={(event) => setText(event.target.value)} />
      </Field>
      {hits.length > 0 ? (
        <ul className="flex flex-col gap-1">
          {hits.map((hit) => (
            <li key={hit.id}>
              <button
                type="button"
                onClick={() => {
                  onPick(hit);
                  setText("");
                }}
                className="flex min-h-11 w-full items-center justify-between gap-3 rounded-control border border-line bg-surface px-3 text-start hover:border-brand"
              >
                <span>{localizedName(hit, lang)}</span>
                <Amount value={hit.sellPrice} className="font-semibold" />
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
