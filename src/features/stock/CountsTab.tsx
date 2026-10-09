import { Plus, X } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { isApprovalCancelled, useApproval } from "@/components/manager-approval";
import { RadioCards } from "@/components/radio-cards";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { ProductPicker } from "@/features/purchases/purchase-lines";
import { formatDate, formatNumber } from "@/lib/format";
import { useCounts, useStartCount } from "@/lib/ipc/stock-m2";
import type { CountScope, CountStatus, StockCount } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { CountScreen, CountStatusTag, useScopeLabel } from "./CountScreen";
import { CategorySelect, LocationSelect, useCan, useLocations, useUserName } from "./stock-parts";

const STATUSES: readonly CountStatus[] = ["open", "closed", "cancelled"];
const SCOPES: readonly CountScope[] = ["full", "category", "products"];

const col = columnHelper<StockCount>();

/** STK-009/010: the stock counts, newest first; one opens its counting screen. */
export function CountsTab() {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const scopeLabel = useScopeLabel();
  const userName = useUserName();
  const { nameOf } = useLocations();
  const counts = useCounts();
  const [status, setStatus] = useState<CountStatus | "">("");
  const [search, setSearch] = useState("");
  const [openedId, setOpenedId] = useState<string | null>(null);
  const [started, setStarted] = useState<StockCount | null>(null);
  const [creating, setCreating] = useState(false);
  const opened = counts.data?.find((c) => c.id === openedId) ?? (started?.id === openedId ? started : null);
  const counter = can("stock.count");

  const text = search.trim().toLowerCase();
  const rows = useMemo(
    () =>
      (counts.data ?? []).filter(
        (c) =>
          (status === "" || c.status === status) &&
          (text === "" ||
            [nameOf(c.locationId), scopeLabel(c), userName(c.userId), c.note ?? ""].some((v) =>
              v.toLowerCase().includes(text),
            )),
      ),
    [counts.data, status, text, nameOf, scopeLabel, userName],
  );

  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("startedAt", {
          header: () => t("stock:counts.columns.startedAt"),
          cell: (c) => <bdi dir="ltr">{formatDate(c.getValue(), lang, { withTime: true })}</bdi>,
        }),
        col.accessor((c) => nameOf(c.locationId), {
          id: "location",
          header: () => t("stock:counts.columns.location"),
        }),
        col.accessor((c) => scopeLabel(c), {
          id: "scope",
          header: () => t("stock:counts.columns.scope"),
          cell: (c) => (
            <span className="inline-flex items-center gap-2">
              {c.getValue()}
              {c.row.original.blind ? <Badge tone="warn">{t("stock:counts.blindTag")}</Badge> : null}
            </span>
          ),
        }),
        col.accessor("counted", {
          header: () => t("stock:counts.columns.progress"),
          meta: { align: "end" },
          cell: (c) => (
            <bdi dir="ltr">
              {t("stock:counts.progress", {
                counted: formatNumber(c.getValue(), lang),
                total: formatNumber(c.row.original.lines, lang),
              })}
            </bdi>
          ),
        }),
        col.accessor("status", {
          header: () => t("stock:counts.columns.status"),
          cell: (c) => <CountStatusTag status={c.getValue()} />,
        }),
        col.accessor((c) => userName(c.userId), {
          id: "startedBy",
          header: () => t("stock:counts.columns.startedBy"),
        }),
      ]),
    [t, lang, nameOf, scopeLabel, userName],
  );

  if (opened) return <CountScreen count={opened} onBack={() => setOpenedId(null)} />;

  return (
    <div className="flex flex-col gap-4">
      {counter ? (
        <Button variant="primary" className="self-end" onClick={() => setCreating(true)}>
          <Plus aria-hidden />
          {t("stock:counts.new")}
        </Button>
      ) : null}
      <TableToolbar
        search={search}
        onSearch={setSearch}
        exportTarget={null}
        filters={
          <Field id="counts-status" label={t("stock:fields.status")}>
            <NativeSelect
              id="counts-status"
              value={status}
              onChange={(event) => setStatus(STATUSES.find((s) => s === event.target.value) ?? "")}
            >
              <option value="">{t("stock:fields.allStatuses")}</option>
              {STATUSES.map((s) => (
                <option key={s} value={s}>
                  {t(`stock:counts.status.${s}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
        }
      />
      <DataTable
        ariaLabel={t("stock:counts.list")}
        data={rows}
        columns={columns}
        getRowId={(c) => c.id}
        // the lines of a count are read with `stock.count` only
        onRowClick={counter ? (c) => setOpenedId(c.id) : undefined}
        empty={counts.isPending ? t("common:status.loading") : t("stock:counts.empty")}
      />
      {creating ? (
        <NewCountDialog
          onClose={() => setCreating(false)}
          onStarted={(count) => {
            setCreating(false);
            setStarted(count);
            setOpenedId(count.id);
          }}
        />
      ) : null}
    </div>
  );
}

/** "Nouvel inventaire": a location, what to count (everything, a category, chosen products) and
 * whether the count is blind. */
function NewCountDialog({
  onClose,
  onStarted,
}: {
  onClose: () => void;
  onStarted: (count: StockCount) => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const approval = useApproval();
  const start = useStartCount();
  const { active, defaultId } = useLocations();
  const [location, setLocation] = useState("");
  const [scope, setScope] = useState<CountScope>("full");
  const [category, setCategory] = useState("");
  const [products, setProducts] = useState<{ id: string; name: string }[]>([]);
  const [blind, setBlind] = useState(false);
  const [note, setNote] = useState("");
  const [missing, setMissing] = useState<"category" | "products" | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const locationId = location === "" ? defaultId : location;

  const submit = () => {
    const lacking = scope === "category" && category === "" ? "category" : null;
    const empty = scope === "products" && products.length === 0 ? "products" : null;
    setMissing(lacking ?? empty);
    setProblem(null);
    if (lacking || empty) return;
    approval
      .run(() =>
        start.mutateAsync({
          // none: the default location (a counter without `stock.view` cannot list them)
          locationId,
          scope,
          categoryId: scope === "category" ? category : null,
          productIds: scope === "products" ? products.map((p) => p.id) : [],
          blind,
          note: note.trim() === "" ? null : note.trim(),
        }),
      )
      .then(onStarted)
      .catch((error: unknown) => {
        if (!isApprovalCancelled(error)) setProblem(errorText(error));
      });
  };

  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("stock:counts.new")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={start.isPending || approval.pending} onClick={submit}>
            {t("stock:counts.start")}
          </Button>
        </>
      }
    >
      {active.length > 0 ? (
        <LocationSelect
          id="count-location"
          label={t("stock:fields.location")}
          value={locationId ?? ""}
          onChange={setLocation}
        />
      ) : null}
      <RadioCards<CountScope>
        legend={t("stock:counts.scope.legend")}
        name="count-scope"
        value={scope}
        onChange={(value) => {
          setScope(value);
          setMissing(null);
        }}
        options={SCOPES.map((s) => ({
          value: s,
          label: t(`stock:counts.scope.${s}`),
          hint: t(`stock:counts.scopeHint.${s}`),
        }))}
      />
      {scope === "category" ? (
        <CategorySelect
          id="count-category"
          value={category}
          onChange={(id) => {
            setCategory(id);
            setMissing(null);
          }}
          allLabel={t("stock:counts.chooseCategory")}
          error={missing === "category" ? t("validation:required") : undefined}
        />
      ) : null}
      {scope === "products" ? (
        <div className="flex flex-col gap-2">
          <ProductPicker
            onPick={(p) => {
              setMissing(null);
              setProducts((all) =>
                all.some((x) => x.id === p.productId) ? all : [...all, { id: p.productId, name: p.name }],
              );
            }}
          />
          {products.length > 0 ? (
            <ul className="flex flex-wrap gap-2">
              {products.map((p) => (
                <li
                  key={p.id}
                  className="inline-flex min-h-11 items-center gap-1 rounded-full bg-surface-2 ps-3 font-semibold"
                >
                  {p.name}
                  <button
                    type="button"
                    aria-label={t("stock:counts.removeProduct", { name: p.name })}
                    onClick={() => setProducts((all) => all.filter((x) => x.id !== p.id))}
                    className="grid size-11 place-items-center rounded-full text-ink-2 hover:bg-surface"
                  >
                    <X aria-hidden className="size-4" />
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
          {missing === "products" ? (
            <p role="alert" className="text-sm text-danger">
              {t("stock:counts.noProducts")}
            </p>
          ) : null}
        </div>
      ) : null}
      <label className="flex min-h-11 items-start gap-3">
        <Checkbox className="mt-1" checked={blind} onCheckedChange={(v) => setBlind(v === true)} />
        <span className="flex flex-col">
          <span className="font-semibold">{t("stock:counts.blind")}</span>
          <span className="text-sm text-ink-2">{t("stock:counts.blindHint")}</span>
        </span>
      </label>
      <Field id="count-note" label={t("stock:fields.note")} optional={t("stock:fields.optional")}>
        <Input
          id="count-note"
          dir="auto"
          maxLength={500}
          value={note}
          onChange={(e) => setNote(e.target.value)}
        />
      </Field>
      {problem ? (
        <p role="alert" className="text-danger">
          {problem}
        </p>
      ) : null}
      {approval.dialog}
    </Dialog>
  );
}
