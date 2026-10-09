import { Archive, Pencil, Plus } from "lucide-react";
import { type FormEvent, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { formatNumber } from "@/lib/format";
import { useArchiveExpenseCategory, useExpenseCategories, useSaveExpenseCategory } from "@/lib/ipc/expenses";
import type { ExpenseCategory, ExpenseCategoryInfo, ExpenseCategoryInput } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { useTillLink } from "@/lib/ipc/lan";

/** M1's cash groups: the line of the X/Z report a till expense of the category adds to. */
const CASH_GROUPS: readonly ExpenseCategory[] = [
  "rent",
  "electricity",
  "water",
  "wages",
  "transport",
  "supplies",
  "maintenance",
  "other",
];
const NAMES = ["nameFr", "nameAr", "nameEn"] as const;

const col = columnHelper<ExpenseCategoryInfo>();

/**
 * `/settings/expense-categories` (FIN-001; `settings.company`): the expense categories with their
 * names in the three languages, the cash group the X/Z report files them under, their order, and
 * their archive (never a deletion: past expenses keep their category).
 */
export function ExpenseCategoriesSettings() {
  const { t } = useTranslation();
  const lang = useLang();
  const [showArchived, setShowArchived] = useState(false);
  const categories = useExpenseCategories(showArchived);
  const [editing, setEditing] = useState<ExpenseCategoryInfo | "new" | null>(null);
  const [archiving, setArchiving] = useState<ExpenseCategoryInfo | null>(null);
  // The categories are the main PC's; on a till they are read-only.
  const till = useTillLink();
  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("sortOrder", {
          header: () => t("settings:expenseCategories.columns.order"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{formatNumber(c.getValue(), lang)}</bdi>,
        }),
        col.accessor("code", {
          header: () => t("settings:expenseCategories.columns.code"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor((c) => localizedName(c, lang), {
          id: "name",
          header: () => t("settings:expenseCategories.columns.name"),
          cell: ({ row, getValue }) => (
            <span className="flex items-center gap-2">
              <span className="truncate font-semibold">{getValue()}</span>
              {row.original.archived ? <Badge>{t("settings:expenseCategories.archived")}</Badge> : null}
            </span>
          ),
        }),
        col.accessor((c) => t(`cash:expense.categories.${c.cashGroup}`), {
          id: "cashGroup",
          header: () => t("settings:expenseCategories.columns.cashGroup"),
        }),
        col.display({
          id: "actions",
          header: () => t("settings:expenseCategories.columns.actions"),
          cell: ({ row }) =>
            row.original.archived || till.isTill ? null : (
              <span className="flex gap-1">
                <Button variant="ghost" onClick={() => setEditing(row.original)}>
                  <Pencil aria-hidden />
                  {t("common:actions.edit")}
                </Button>
                <Button variant="ghost" onClick={() => setArchiving(row.original)}>
                  <Archive aria-hidden />
                  {t("settings:expenseCategories.archive")}
                </Button>
              </span>
            ),
        }),
      ]),
    [t, lang, till.isTill],
  );
  return (
    <div className="flex flex-col gap-4">
      <HostOnlyNote />
      <Card>
        <p className="text-ink-2">{t("settings:expenseCategories.intro")}</p>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <label className="flex min-h-11 items-center gap-2">
            <Checkbox
              checked={showArchived}
              onCheckedChange={(checked) => setShowArchived(checked === true)}
            />
            {t("settings:expenseCategories.showArchived")}
          </label>
          {till.isTill ? null : (
            <Button variant="primary" onClick={() => setEditing("new")}>
              <Plus aria-hidden />
              {t("settings:expenseCategories.new")}
            </Button>
          )}
        </div>
      </Card>
      <DataTable
        ariaLabel={t("settings:sections.expenseCategories")}
        data={categories.data ?? []}
        columns={columns}
        getRowId={(c) => c.id}
        rowHeight={52}
        empty={categories.isPending ? t("common:status.loading") : undefined}
      />
      {editing ? (
        <CategoryDialog
          category={editing === "new" ? null : editing}
          nextOrder={Math.max(0, ...(categories.data ?? []).map((c) => c.sortOrder)) + 1}
          onClose={() => setEditing(null)}
        />
      ) : null}
      {archiving ? <ArchiveDialog category={archiving} onClose={() => setArchiving(null)} /> : null}
    </div>
  );
}

type Errors = Partial<Record<"code" | "name" | "sortOrder", string>>;

function CategoryDialog({
  category,
  nextOrder,
  onClose,
}: {
  category: ExpenseCategoryInfo | null;
  nextOrder: number;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const save = useSaveExpenseCategory();
  const [code, setCode] = useState(category?.code ?? "");
  const [names, setNames] = useState({
    nameFr: category?.nameFr ?? "",
    nameAr: category?.nameAr ?? "",
    nameEn: category?.nameEn ?? "",
  });
  const [cashGroup, setCashGroup] = useState<ExpenseCategory>(category?.cashGroup ?? "other");
  const [order, setOrder] = useState(String(category?.sortOrder ?? nextOrder));
  const [errors, setErrors] = useState<Errors>({});
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const found: Errors = {};
    if (code.trim() === "") found.code = "validation:required";
    if (NAMES.every((k) => names[k].trim() === "")) found.name = "validation:nameRequired";
    const sortOrder = Number.parseInt(order, 10);
    if (!Number.isSafeInteger(sortOrder)) found.sortOrder = "validation:required";
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    const input: ExpenseCategoryInput = {
      code: code.trim(),
      nameAr: names.nameAr.trim(),
      nameFr: names.nameFr.trim(),
      nameEn: names.nameEn.trim(),
      cashGroup,
      sortOrder,
    };
    save.mutate(
      { id: category?.id ?? null, input },
      {
        onSuccess: () => {
          toast.success(t("settings:expenseCategories.saved"));
          onClose();
        },
      },
    );
  };
  const title = category ? t("settings:expenseCategories.edit") : t("settings:expenseCategories.new");
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={title}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="expense-category" variant="primary" disabled={save.isPending}>
            {t("settings:expenseCategories.save")}
          </Button>
        </>
      }
    >
      <form id="expense-category" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <div className="grid gap-4 sm:grid-cols-2">
          <Field
            id="expense-category-code"
            label={t("settings:expenseCategories.fields.code")}
            error={errors.code ? t(errors.code) : undefined}
          >
            <GuardedInput
              id="expense-category-code"
              dir="ltr"
              maxLength={32}
              aria-invalid={Boolean(errors.code)}
              value={code}
              onValueChange={setCode}
            />
          </Field>
          <Field
            id="expense-category-order"
            label={t("settings:expenseCategories.fields.sortOrder")}
            error={errors.sortOrder ? t(errors.sortOrder) : undefined}
          >
            <NumericInput id="expense-category-order" kind="integer" value={order} onValueChange={setOrder} />
          </Field>
        </div>
        <div className="grid gap-4 sm:grid-cols-3">
          {NAMES.map((k) => (
            <Field key={k} id={`expense-category-${k}`} label={t(`products:fields.${k}`)}>
              <Input
                id={`expense-category-${k}`}
                dir={k === "nameAr" ? "rtl" : "auto"}
                maxLength={80}
                aria-invalid={Boolean(errors.name)}
                value={names[k]}
                onChange={(event) => setNames({ ...names, [k]: event.target.value })}
              />
            </Field>
          ))}
        </div>
        {errors.name ? (
          <p role="alert" className="text-sm text-danger">
            {t(errors.name)}
          </p>
        ) : null}
        <Field id="expense-category-group" label={t("settings:expenseCategories.fields.cashGroup")}>
          <NativeSelect
            id="expense-category-group"
            value={cashGroup}
            onChange={(event) => setCashGroup(event.target.value as ExpenseCategory)}
          >
            {CASH_GROUPS.map((group) => (
              <option key={group} value={group}>
                {t(`cash:expense.categories.${group}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        {save.error ? (
          <p role="alert" className="text-danger">
            {errorText(save.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}

function ArchiveDialog({ category, onClose }: { category: ExpenseCategoryInfo; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const archive = useArchiveExpenseCategory();
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("settings:expenseCategories.archiveTitle")}
      description={t("settings:expenseCategories.archiveBody", { name: localizedName(category, lang) })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="danger"
            disabled={archive.isPending}
            onClick={() => archive.mutate(category.id, { onSuccess: onClose })}
          >
            {t("settings:expenseCategories.archive")}
          </Button>
        </>
      }
    >
      {archive.error ? (
        <p role="alert" className="text-danger">
          {errorText(archive.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
