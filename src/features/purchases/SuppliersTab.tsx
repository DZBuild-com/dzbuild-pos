import { zodResolver } from "@hookform/resolvers/zod";
import { Plus } from "lucide-react";
import { useMemo, useState } from "react";
import { Controller, useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { toast } from "sonner";
import { z } from "zod";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TruncatedNotice, capped } from "@/components/data-table/list-notices";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { NumericInput } from "@/components/numeric-input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { toAsciiDigits } from "@/lib/digits";
import { useDatapack } from "@/lib/ipc/app";
import { useSession } from "@/lib/ipc/auth";
import { errorKey, toAppError } from "@/lib/ipc/client";
import { supplierQuery, useCreateSupplier, useSupplierSearch, useUpdateSupplier } from "@/lib/ipc/suppliers";
import type { ExportTarget, Supplier, SupplierHit, SupplierInput } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

const schema = z.object({
  name: z
    .string()
    .refine((v) => v.trim() !== "", "validation:required")
    .refine((v) => v.trim().length <= 120, "validation:tooLong"),
  phone: z.string(),
  phone2: z.string(),
  email: z.string(),
  address: z.string(),
  wilaya: z.string(),
  nif: z.string(),
  nis: z.string(),
  rc: z.string(),
  ai: z.string(),
  nin: z.string(),
  rib: z.string(),
  paymentTermsDays: z
    .string()
    .refine(
      (v) => v.trim() === "" || (/^\d{1,3}$/.test(v.trim()) && Number(v) <= 365),
      "validation:daysRange",
    ),
  notes: z.string(),
});

type Values = z.infer<typeof schema>;
type Name = keyof Values;

/** The backend's field names (`AppError.field`) are the form's. */
const FIELDS: readonly Name[] = [
  "name",
  "phone",
  "phone2",
  "email",
  "address",
  "wilaya",
  "nif",
  "nis",
  "rc",
  "ai",
  "nin",
  "rib",
  "paymentTermsDays",
  "notes",
];

function toValues(s: Supplier | null): Values {
  return {
    name: s?.name ?? "",
    phone: s?.phone ?? "",
    phone2: s?.phone2 ?? "",
    email: s?.email ?? "",
    address: s?.address ?? "",
    wilaya: s?.wilaya ?? "",
    nif: s?.nif ?? "",
    nis: s?.nis ?? "",
    rc: s?.rc ?? "",
    ai: s?.ai ?? "",
    nin: s?.nin ?? "",
    rib: s?.rib ?? "",
    paymentTermsDays: s?.paymentTermsDays != null ? String(s.paymentTermsDays) : "",
    notes: s?.notes ?? "",
  };
}

const opt = (v: string) => (v.trim() === "" ? null : v.trim());
/** Phones, ids and the RIB typed with Arabic-Indic digits are sent with Western ones. */
const digits = (v: string) => opt(toAsciiDigits(v));

function toInput(v: Values, before: Supplier | null): SupplierInput {
  return {
    code: before?.code ?? null,
    name: v.name.trim(),
    phone: digits(v.phone),
    phone2: digits(v.phone2),
    email: opt(v.email),
    address: opt(v.address),
    wilaya: opt(v.wilaya),
    nif: digits(v.nif),
    nis: digits(v.nis),
    rc: opt(v.rc),
    ai: opt(v.ai),
    nin: digits(v.nin),
    rib: digits(v.rib),
    paymentTermsDays: v.paymentTermsDays.trim() === "" ? null : Number(toAsciiDigits(v.paymentTermsDays)),
    notes: opt(v.notes),
  };
}

/**
 * PUR-001: creates or edits a supplier, as the customer form does: identity, tax ids, the RIB and
 * the payment terms. The backend cleans every field and names the one it refuses.
 */
export function SupplierForm({
  supplier,
  onClose,
  onSaved,
}: {
  supplier: Supplier | null;
  onClose: () => void;
  onSaved?: (supplier: Supplier) => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const wilayas = useDatapack().data?.wilayas ?? [];
  const create = useCreateSupplier();
  const update = useUpdateSupplier();
  const [failure, setFailure] = useState<unknown>(null);
  const { register, control, handleSubmit, formState, setError } = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: toValues(supplier),
  });
  const err = (name: Name) => {
    const message = formState.errors[name]?.message;
    return typeof message === "string" ? t(message) : undefined;
  };
  const refused = (error: unknown) => {
    const field = FIELDS.find((f) => f === toAppError(error).field);
    if (field) {
      setError(
        field,
        { type: "server", message: errorKey(error, (k) => i18n.exists(k)) },
        { shouldFocus: true },
      );
    } else {
      setFailure(error);
    }
  };
  const saved = (s: Supplier) => {
    toast.success(t("purchases:supplier.saved"));
    onClose();
    onSaved?.(s);
  };
  const submit = handleSubmit((values) => {
    setFailure(null);
    const input = toInput(values, supplier);
    if (supplier) update.mutate({ id: supplier.id, input }, { onSuccess: saved, onError: refused });
    else create.mutate(input, { onSuccess: saved, onError: refused });
  });
  const text = (name: Name, labelKey: string, props: { dir?: "ltr" | "auto"; type?: string } = {}) => (
    <Field
      id={`supplier-${name}`}
      label={t(labelKey)}
      optional={t("customers:form.optional")}
      error={err(name)}
    >
      <Input
        id={`supplier-${name}`}
        dir={props.dir ?? "auto"}
        type={props.type ?? "text"}
        aria-invalid={Boolean(err(name))}
        {...register(name)}
      />
    </Field>
  );

  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={
        supplier
          ? t("purchases:supplier.editTitle", { name: supplier.name })
          : t("purchases:supplier.newTitle")
      }
      description={supplier?.code}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            type="submit"
            form="supplier-form"
            variant="primary"
            disabled={create.isPending || update.isPending}
          >
            {t("purchases:supplier.save")}
          </Button>
        </>
      }
    >
      <form id="supplier-form" noValidate onSubmit={(e) => void submit(e)} className="flex flex-col gap-6">
        <section className="flex flex-col gap-4">
          <h3 className="text-lg font-bold">{t("customers:form.identity")}</h3>
          <div className="grid gap-4 md:grid-cols-2">
            <Field id="supplier-name" label={t("customers:fields.name")} error={err("name")}>
              <Input
                id="supplier-name"
                dir="auto"
                autoFocus
                aria-invalid={Boolean(err("name"))}
                {...register("name")}
              />
            </Field>
            {text("phone", "customers:fields.phone", { dir: "ltr", type: "tel" })}
            {text("phone2", "customers:fields.phone2", { dir: "ltr", type: "tel" })}
            {text("email", "customers:fields.email", { dir: "ltr", type: "email" })}
            <Field id="supplier-wilaya" label={t("customers:fields.wilaya")} error={err("wilaya")}>
              <NativeSelect id="supplier-wilaya" {...register("wilaya")}>
                <option value="">{t("setup:shop.wilayaNone")}</option>
                {wilayas.map((w) => (
                  <option key={w.code} value={w.code}>
                    {`${w.code} ${localizedName(w, lang)}`}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          </div>
          {text("address", "customers:fields.address")}
        </section>
        <section className="flex flex-col gap-4">
          <h3 className="text-lg font-bold">{t("customers:form.tax")}</h3>
          <div className="grid gap-4 md:grid-cols-2">
            {text("nif", "customers:fields.nif", { dir: "ltr" })}
            {text("nis", "customers:fields.nis", { dir: "ltr" })}
            {text("rc", "customers:fields.rc", { dir: "ltr" })}
            {text("ai", "customers:fields.ai", { dir: "ltr" })}
            {text("nin", "customers:fields.nin", { dir: "ltr" })}
          </div>
        </section>
        <section className="flex flex-col gap-4">
          <h3 className="text-lg font-bold">{t("purchases:supplier.terms")}</h3>
          <div className="grid gap-4 md:grid-cols-2">
            {text("rib", "purchases:supplier.fields.rib", { dir: "ltr" })}
            <Field
              id="supplier-paymentTermsDays"
              label={t("purchases:supplier.fields.paymentTerms")}
              optional={t("customers:form.optional")}
              hint={t("purchases:supplier.paymentTermsHint")}
              error={err("paymentTermsDays")}
            >
              <Controller
                control={control}
                name="paymentTermsDays"
                render={({ field }) => (
                  <NumericInput
                    id="supplier-paymentTermsDays"
                    kind="integer"
                    aria-invalid={Boolean(err("paymentTermsDays"))}
                    value={field.value}
                    onValueChange={field.onChange}
                  />
                )}
              />
            </Field>
          </div>
          <Field
            id="supplier-notes"
            label={t("customers:fields.notes")}
            optional={t("customers:form.optional")}
            error={err("notes")}
          >
            <textarea
              id="supplier-notes"
              dir="auto"
              rows={2}
              maxLength={1000}
              className="w-full rounded-control border border-line bg-surface px-3 py-2 text-base text-ink"
              {...register("notes")}
            />
          </Field>
        </section>
        {failure ? (
          <p role="alert" className="text-danger">
            {errorText(failure)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}

type Filter = "owed" | "archived";
const FILTERS: readonly Filter[] = ["owed", "archived"];

const col = columnHelper<SupplierHit>();

/** Suppliers listed at most; a search or a filter finds the others. */
const LIST_LIMIT = 500;

/** PUR-001: the supplier directory with what the shop owes each; a row opens the supplier. */
export function SuppliersTab() {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const manages = useSession().data?.permissions.includes("suppliers.manage") ?? false;
  const wilayas = useDatapack().data?.wilayas;
  const [search, setSearch] = useState("");
  const [filters, setFilters] = useState<Filter[]>([]);
  const [creating, setCreating] = useState(false);
  const text = useDebounced(search.trim(), 200);
  const on = (f: Filter) => filters.includes(f);
  const query = supplierQuery({ text, withBalance: on("owed"), includeArchived: on("archived") });
  // One more than shown: whether the list stops at its limit (the export takes the whole file).
  const hits = useSupplierSearch({ ...query, limit: LIST_LIMIT + 1 });
  const list = capped(hits.data, LIST_LIMIT);
  const exportTarget: ExportTarget | null = manages ? { kind: "suppliers", query } : null;

  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("code", {
          header: () => t("customers:fields.code"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("name", {
          header: () => t("customers:fields.name"),
          cell: ({ row }) => (
            <span className="flex items-center gap-2">
              <span className="truncate font-semibold">{row.original.name}</span>
              {row.original.archived ? <Badge>{t("customers:status.archived")}</Badge> : null}
            </span>
          ),
        }),
        col.accessor("phone", {
          header: () => t("customers:fields.phone"),
          enableSorting: false,
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("wilaya", {
          header: () => t("customers:fields.wilaya"),
          cell: (c) => {
            const code = c.getValue();
            const wilaya = code ? wilayas?.find((w) => w.code === code) : undefined;
            return wilaya ? `${wilaya.code} ${localizedName(wilaya, lang)}` : code;
          },
        }),
        col.accessor("balance", {
          header: () => t("purchases:supplier.balance"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} className={cn(c.getValue() > 0 && "font-semibold")} />,
        }),
      ]),
    [t, lang, wilayas],
  );

  return (
    <div className="flex flex-col gap-4">
      {manages ? (
        <div className="flex justify-end">
          <Button variant="primary" onClick={() => setCreating(true)}>
            <Plus aria-hidden />
            {t("purchases:supplier.new")}
          </Button>
        </div>
      ) : null}
      <TableToolbar
        search={search}
        onSearch={setSearch}
        exportTarget={exportTarget}
        filters={
          <div role="group" aria-label={t("customers:filters.label")} className="flex flex-wrap gap-2">
            {FILTERS.map((f) => (
              <Button
                key={f}
                variant="outline"
                aria-pressed={on(f)}
                className={cn(on(f) && "border-brand bg-brand-soft hover:bg-brand-soft")}
                onClick={() => setFilters((all) => (on(f) ? all.filter((x) => x !== f) : [...all, f]))}
              >
                {t(`purchases:supplier.filters.${f}`)}
              </Button>
            ))}
          </div>
        }
      />
      {list.truncated ? <TruncatedNotice rows={LIST_LIMIT} /> : null}
      <DataTable
        ariaLabel={t("purchases:tabs.suppliers")}
        data={list.rows}
        columns={columns}
        sortable={!list.truncated}
        getRowId={(row) => row.id}
        onRowClick={(row) => void navigate(`/purchases/suppliers/${row.id}`)}
        empty={hits.isPending ? t("common:status.loading") : t("purchases:supplier.empty")}
      />
      {creating ? (
        <SupplierForm
          supplier={null}
          onClose={() => setCreating(false)}
          onSaved={(s) => void navigate(`/purchases/suppliers/${s.id}`)}
        />
      ) : null}
    </div>
  );
}
