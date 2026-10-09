import { FileUp } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { Button } from "@/components/ui/button";
import { useCan } from "@/features/stock/stock-parts";
import type { ImportEntity, Permission } from "@/lib/ipc/types";

/** Every import, in the order the lists show them (pos-core `ImportEntity::ALL`). */
export const IMPORT_ENTITIES: readonly ImportEntity[] = [
  "products",
  "categories",
  "opening_stock",
  "customers",
  "customer_balances",
  "suppliers",
  "supplier_balances",
  "count_lines",
];

/** Who may run each import (pos-core `ImportEntity::permission`); Rust checks it again. */
export const IMPORT_PERMISSION: Record<ImportEntity, Permission> = {
  products: "products.import",
  categories: "products.import",
  opening_stock: "products.import",
  customers: "customers.manage",
  customer_balances: "customers.manage",
  suppliers: "suppliers.manage",
  supplier_balances: "suppliers.manage",
  count_lines: "stock.count",
};

/** Imports that post opening entries (stock, balances, counted quantities): they only add, so no
 * policy applies to them (pos-core `ImportEntity::posts`). */
export const POSTING_IMPORTS: ReadonlySet<ImportEntity> = new Set([
  "opening_stock",
  "customer_balances",
  "supplier_balances",
  "count_lines",
]);

export function isImportEntity(value: string | undefined): value is ImportEntity {
  return IMPORT_ENTITIES.some((e) => e === value);
}

/** The imports a set of permissions allows, in list order. */
export function importableEntities(can: (permission: Permission) => boolean): ImportEntity[] {
  return IMPORT_ENTITIES.filter((e) => can(IMPORT_PERMISSION[e]));
}

/** The "Importer" link of a list screen: the import wizard of the first of `entities` the user may
 * run (the wizard page offers the others), else nothing. */
export function ImportLink({ entities }: { entities: readonly ImportEntity[] }) {
  const { t } = useTranslation();
  const can = useCan();
  const entity = entities.find((e) => can(IMPORT_PERMISSION[e]));
  if (!entity) return null;
  return (
    <Button asChild variant="outline">
      <Link to={`/imports/${entity}`}>
        <FileUp aria-hidden />
        {t("imports:title")}
      </Link>
    </Button>
  );
}
