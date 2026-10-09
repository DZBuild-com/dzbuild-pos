import { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { useSession, useUserTiles } from "@/lib/ipc/auth";
import { useCategories, useUnits } from "@/lib/ipc/catalog";
import { useStockLocations } from "@/lib/ipc/stock";
import type { Permission, StockLocation } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useLang } from "@/lib/use-language";

/** Whether the logged-in user holds a permission: the UI hides what a role cannot do, and Rust
 * re-checks every command anyway (spec §4.5). */
export function useCan(): (permission: Permission) => boolean {
  const permissions = useSession().data?.permissions;
  return useCallback((permission: Permission) => permissions?.includes(permission) ?? false, [permissions]);
}

/** The active stock locations, the default one first, and any location's name in the UI
 * language ("" while unknown: a role without `stock.view` cannot read them). */
export function useLocations(): {
  active: StockLocation[];
  defaultId: string | null;
  nameOf: (id: string) => string;
} {
  const lang = useLang();
  const locations = useStockLocations().data;
  return useMemo(() => {
    const all = locations ?? [];
    const active = all.filter((l) => !l.archived).sort((a, b) => Number(b.isDefault) - Number(a.isDefault));
    const nameOf = (id: string) => {
      const found = all.find((l) => l.id === id);
      return found ? localizedName(found, lang) : "";
    };
    return { active, defaultId: active.find((l) => l.isDefault)?.id ?? null, nameOf };
  }, [locations, lang]);
}

/** A user's display name by id (the login tiles), "" when unknown. */
export function useUserName(): (id: string | null) => string {
  const tiles = useUserTiles().data;
  return useCallback((id: string | null) => tiles?.find((u) => u.id === id)?.displayName ?? "", [tiles]);
}

/** A unit's name in the UI language by its code ("pc" until the units are read). */
export function useUnitName(): (code: string) => string {
  const lang = useLang();
  const units = useUnits().data;
  return useCallback(
    (code: string) => {
      const unit = units?.find((u) => u.code === code);
      return unit ? localizedName(unit, lang) : code;
    },
    [units, lang],
  );
}

/** A location of the shop; `allLabel` adds the empty choice ("Tous les emplacements"). */
export function LocationSelect({
  id,
  label,
  value,
  onChange,
  allLabel,
  exclude,
  error,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (locationId: string) => void;
  allLabel?: string;
  /** A location that cannot be chosen here (the other end of a transfer). */
  exclude?: string;
  error?: string;
}) {
  const { active, nameOf } = useLocations();
  return (
    <Field id={id} label={label} error={error}>
      <NativeSelect
        id={id}
        value={value}
        aria-invalid={Boolean(error)}
        onChange={(event) => onChange(event.target.value)}
      >
        {allLabel !== undefined ? <option value="">{allLabel}</option> : null}
        {active
          .filter((l) => l.id !== exclude)
          .map((l) => (
            <option key={l.id} value={l.id}>
              {nameOf(l.id)}
            </option>
          ))}
      </NativeSelect>
    </Field>
  );
}

/** A product category, subcategories included by the backend; `allLabel` is the empty choice. */
export function CategorySelect({
  id,
  value,
  onChange,
  allLabel,
  error,
}: {
  id: string;
  value: string;
  onChange: (categoryId: string) => void;
  allLabel: string;
  error?: string;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const categories = useCategories().data ?? [];
  return (
    <Field id={id} label={t("stock:fields.category")} error={error}>
      <NativeSelect
        id={id}
        value={value}
        aria-invalid={Boolean(error)}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="">{allLabel}</option>
        {categories.map((c) => (
          <option key={c.id} value={c.id}>
            {localizedName(c, lang)}
          </option>
        ))}
      </NativeSelect>
    </Field>
  );
}
