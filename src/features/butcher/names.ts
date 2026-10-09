import { useMemo } from "react";
import { useProductsOf } from "@/lib/ipc/butcher";
import { localizedName } from "@/lib/names";
import { useLang } from "@/lib/use-language";

/** The names of `ids` in the UI language (the carcass and cuts of the cutting templates): `ready`
 * once every product was read, `nameOf` an empty string for a product still loading or gone. */
export function useProductNames(ids: readonly string[]): {
  ready: boolean;
  nameOf: (id: string) => string;
} {
  const lang = useLang();
  const unique = useMemo(() => [...new Set(ids)], [ids]);
  const results = useProductsOf(unique);
  const ready = results.every((r) => !r.isPending);
  const names = new Map(
    unique.map((id, i) => {
      const product = results[i]?.data;
      return [id, product ? localizedName(product, lang) : ""];
    }),
  );
  return { ready, nameOf: (id: string) => names.get(id) ?? "" };
}
