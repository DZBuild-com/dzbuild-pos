import { Archive, Plus } from "lucide-react";
import { useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { useCan } from "@/features/stock/stock-parts";
import { useCategories, useTaxRates, useUnits } from "@/lib/ipc/catalog";
import { useArchiveDepartment, useDepartments, useSaveDepartment } from "@/lib/ipc/packs";
import type { DepartmentKey } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** A key without a colour of its own shows in the till's neutral tile colour. */
const DEFAULT_COLOR = "#FFC300";

const col = columnHelper<DepartmentKey>();

/**
 * SAL-019 / VRT-001: the department keys of the till's calculator (vegetables, fruit, sundries),
 * each selling its department at the price typed at the till: name in the three languages,
 * colour, category, VAT rate, by the piece or by weight, and its place on the panel. Archived,
 * never deleted (its sales name it).
 */
export function DepartmentKeysEditor() {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const errorText = useErrorText();
  const keys = useDepartments();
  const categories = useCategories().data;
  const rates = useTaxRates().data;
  const [open, setOpen] = useState<DepartmentKey | "new" | null>(null);
  const edits = can("products.edit");
  const columns = useMemo(
    () =>
      col.columns([
        col.accessor((k) => localizedName(k, lang), {
          id: "name",
          header: () => t("products:columns.name"),
          cell: (c) => (
            <span className="flex items-center gap-2">
              <span
                aria-hidden
                className="inline-block size-4 shrink-0 rounded-full border border-line"
                style={{ backgroundColor: c.row.original.color ?? DEFAULT_COLOR }}
              />
              {c.getValue()}
            </span>
          ),
        }),
        col.accessor(
          (k) => {
            const category = categories?.find((c) => c.id === k.categoryId);
            return category ? localizedName(category, lang) : "";
          },
          { id: "category", header: () => t("products:departments.category") },
        ),
        col.accessor(
          (k) => {
            const rate = rates?.find((r) => r.id === k.taxRateId);
            return rate ? localizedName(rate, lang) : "";
          },
          { id: "rate", header: () => t("products:departments.taxRate") },
        ),
        col.accessor("sortOrder", {
          header: () => t("products:departments.order"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
      ]),
    [t, lang, categories, rates],
  );
  return (
    <section className="flex flex-col gap-4" aria-labelledby="departments-title">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex flex-col gap-1">
          <h2 id="departments-title" className="text-2xl font-bold">
            {t("products:departments.title")}
          </h2>
          <p className="text-ink-2">{t("products:departments.intro")}</p>
        </div>
        {edits ? (
          <Button variant="primary" onClick={() => setOpen("new")}>
            <Plus aria-hidden />
            {t("products:departments.new")}
          </Button>
        ) : null}
      </div>
      {keys.error ? (
        <p role="alert" className="text-danger">
          {errorText(keys.error)}
        </p>
      ) : null}
      <DataTable
        ariaLabel={t("products:departments.title")}
        data={keys.data ?? []}
        columns={columns}
        getRowId={(k) => k.id}
        onRowClick={edits ? (k) => setOpen(k) : undefined}
        empty={keys.isPending ? t("common:status.loading") : t("products:departments.empty")}
      />
      {open ? <KeyDialog current={open === "new" ? null : open} onClose={() => setOpen(null)} /> : null}
    </section>
  );
}

function KeyDialog({ current, onClose }: { current: DepartmentKey | null; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const id = useId();
  const categories = (useCategories().data ?? []).filter((c) => !c.archived);
  const rates = (useTaxRates().data ?? []).filter((r) => r.active);
  const kg = useUnits().data?.find((u) => u.code === "kg");
  const save = useSaveDepartment();
  const archive = useArchiveDepartment();
  const [names, setNames] = useState({
    nameFr: current?.nameFr ?? "",
    nameAr: current?.nameAr ?? "",
    nameEn: current?.nameEn ?? "",
  });
  const [categoryId, setCategoryId] = useState(current?.categoryId ?? "");
  const [taxRateId, setTaxRateId] = useState(current?.taxRateId ?? "");
  const [byWeight, setByWeight] = useState(kg !== undefined && current?.unitId === kg.id);
  const [color, setColor] = useState(current?.color ?? DEFAULT_COLOR);
  const [order, setOrder] = useState(String(current?.sortOrder ?? 1));
  const [problem, setProblem] = useState<string | null>(null);
  const rate = taxRateId || rates[0]?.id || "";

  const submit = () => {
    if (Object.values(names).some((n) => n.trim() === "") || rate === "") {
      setProblem(t("products:departments.missing"));
      return;
    }
    setProblem(null);
    save.mutate(
      {
        id: current?.id ?? null,
        input: {
          nameFr: names.nameFr.trim(),
          nameAr: names.nameAr.trim(),
          nameEn: names.nameEn.trim(),
          categoryId: categoryId === "" ? null : categoryId,
          taxRateId: rate,
          unitId: byWeight && kg ? kg.id : null,
          color,
          sortOrder: Number(order) || 0,
        },
      },
      {
        onSuccess: () => {
          toast.success(t("products:departments.saved"));
          onClose();
        },
      },
    );
  };
  const archiveIt = () => {
    if (!current) return;
    archive.mutate(current.id, {
      onSuccess: () => {
        toast.success(t("products:departments.archived"));
        onClose();
      },
    });
  };
  const error = save.error ?? archive.error;
  const nameField = (key: keyof typeof names, fieldLang: "fr" | "ar" | "en") => (
    <Field id={`${id}-${key}`} label={t(`products:departments.${key}`)}>
      <Input
        id={`${id}-${key}`}
        lang={fieldLang}
        dir={fieldLang === "ar" ? "rtl" : "ltr"}
        maxLength={60}
        value={names[key]}
        onChange={(event) => setNames((all) => ({ ...all, [key]: event.target.value }))}
      />
    </Field>
  );
  return (
    <Dialog
      open
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={
        current
          ? t("products:departments.edit", { name: localizedName(current, lang) })
          : t("products:departments.new")
      }
      footer={
        <>
          {current ? (
            <Button variant="danger" disabled={archive.isPending} onClick={archiveIt}>
              <Archive aria-hidden />
              {t("products:departments.archive")}
            </Button>
          ) : null}
          <Button variant="primary" disabled={save.isPending} onClick={submit}>
            {t("products:departments.save")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 sm:grid-cols-2">
        {nameField("nameFr", "fr")}
        {nameField("nameAr", "ar")}
        {nameField("nameEn", "en")}
        <Field id={`${id}-category`} label={t("products:departments.category")}>
          <NativeSelect
            id={`${id}-category`}
            value={categoryId}
            onChange={(event) => setCategoryId(event.target.value)}
          >
            <option value="">{t("products:departments.noCategory")}</option>
            {categories.map((c) => (
              <option key={c.id} value={c.id}>
                {localizedName(c, lang)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id={`${id}-rate`} label={t("products:departments.taxRate")}>
          <NativeSelect id={`${id}-rate`} value={rate} onChange={(event) => setTaxRateId(event.target.value)}>
            {rates.map((r) => (
              <option key={r.id} value={r.id}>
                {localizedName(r, lang)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id={`${id}-color`} label={t("products:departments.color")}>
          <Input
            id={`${id}-color`}
            type="color"
            dir="ltr"
            className="w-24 p-1"
            value={color}
            onChange={(event) => setColor(event.target.value.toUpperCase())}
          />
        </Field>
        <Field id={`${id}-order`} label={t("products:departments.order")}>
          <NumericInput
            id={`${id}-order`}
            kind="integer"
            className="w-28"
            value={order}
            onValueChange={setOrder}
          />
        </Field>
        {kg ? (
          <label className="flex min-h-11 items-center gap-3 font-semibold sm:col-span-2">
            <Checkbox checked={byWeight} onCheckedChange={(v) => setByWeight(v === true)} />
            {t("products:departments.byWeight")}
          </label>
        ) : null}
      </div>
      {problem ? (
        <p role="alert" className="text-danger">
          {problem}
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="text-danger">
          {errorText(error)}
        </p>
      ) : null}
    </Dialog>
  );
}
