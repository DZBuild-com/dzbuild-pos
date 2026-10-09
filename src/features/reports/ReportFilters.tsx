import { X } from "lucide-react";
import { type KeyboardEvent, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { DateRangePicker } from "@/components/date-range";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { SupplierSelect } from "@/features/purchases/purchase-lines";
import { CategorySelect, LocationSelect, useLocations } from "@/features/stock/stock-parts";
import { type DateRange, daysBetween, isDay, rangeOf, today } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useSession, useUserTiles } from "@/lib/ipc/auth";
import { useCustomerSearch } from "@/lib/ipc/customers";
import { useProductSearch } from "@/lib/ipc/products";
import { usePromotions } from "@/lib/ipc/promotions";
import { useCounts } from "@/lib/ipc/stock-m2";
import type { Grouping, ReportFilter, ReportInfo, ReportRequest } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useDebounced } from "@/lib/use-debounced";
import { useLang } from "@/lib/use-language";

const GROUPINGS: readonly Grouping[] = ["day", "week", "month", "year"];

/** Beyond this many days a period is grouped by week, beyond a year by month, until the user
 * picks a grouping: a year by day would draw about 365 bars. */
const DAYS_BY_DAY = 62;
const DAYS_BY_WEEK = 366;

/** The grouping a period gets when the user has not picked one. */
export function defaultGrouping(range: DateRange): Grouping {
  if (!isDay(range.from) || !isDay(range.to) || range.from > range.to) return "day";
  const days = daysBetween(range.from, range.to);
  return days <= DAYS_BY_DAY ? "day" : days <= DAYS_BY_WEEK ? "week" : "month";
}

/** The order the filters are shown in (after the one the report cannot run without). */
const ORDER: readonly ReportFilter[] = [
  "grouping",
  "terminal",
  "user",
  "category",
  "product",
  "customer",
  "location",
  "supplier",
  "promotion",
  "count",
];

/** The request field of every filter but the grouping. */
const FIELDS = {
  terminal: "terminalId",
  user: "userId",
  category: "categoryId",
  product: "productId",
  customer: "customerId",
  location: "locationId",
  supplier: "supplierId",
  promotion: "promotionId",
  count: "countId",
} as const satisfies Record<Exclude<ReportFilter, "grouping">, keyof ReportRequest>;

/** What the report screen holds: the period of a dated report, the day of an "as of" one, and one
 * id per filter ("" or absent: all), the grouping holding its `Grouping`. */
export interface FilterState {
  range: DateRange;
  asOf: string;
  values: Partial<Record<ReportFilter, string>>;
}

/** The period a link asked for (`?from=…&to=…`, the dashboard's), else this month and today. */
export function initialFilters(params: URLSearchParams): FilterState {
  const day = today();
  const from = params.get("from") ?? "";
  const to = params.get("to") ?? "";
  return {
    range: isDay(from) && isDay(to) ? { from, to } : rangeOf("this_month", day),
    asOf: isDay(to) ? to : day,
    values: {},
  };
}

/** The filter the report needs and the user has not chosen yet. */
export function missingFilter(info: ReportInfo, state: FilterState): ReportFilter | null {
  const required = info.requiredFilter;
  return required !== null && !state.values[required] ? required : null;
}

/** Whether the report shows the "as of" day: kinds that show their own document (a count, a
 * promotion) ignore it. */
function showsAsOf(info: ReportInfo): boolean {
  return !info.dated && info.requiredFilter === null;
}

/**
 * The request of the screen, carrying only the filters chosen; `null` while the period is not two
 * days in order or the required filter is missing (the backend would refuse it with
 * `report_filter_required`, which the user never sees).
 */
export function requestOf(info: ReportInfo, state: FilterState): ReportRequest | null {
  if (missingFilter(info, state)) return null;
  let from = state.asOf;
  let to = state.asOf;
  if (info.dated) {
    ({ from, to } = state.range);
    if (!isDay(from) || !isDay(to) || from > to) return null;
  } else if (!showsAsOf(info)) {
    from = today();
    to = from;
  } else if (!isDay(to)) {
    return null;
  }
  const request: ReportRequest = { kind: info.kind, from, to };
  for (const filter of info.filters) {
    const value = state.values[filter];
    if (filter === "grouping") {
      request.group = GROUPINGS.find((g) => g === value) ?? defaultGrouping(state.range);
    } else if (value) request[FIELDS[filter]] = value;
  }
  return request;
}

/**
 * The filters `ReportInfo.filters` lists, the required one first: the period of a dated report or
 * the "as of" day, the grouping, the terminal, the cashier, the category, a product or customer
 * picked by name, the location, the supplier, the promotion or the stock count.
 */
export function ReportFilters({
  info,
  value,
  onChange,
}: {
  info: ReportInfo;
  value: FilterState;
  onChange: (next: FilterState) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  const required = info.requiredFilter;
  const others = ORDER.filter((f) => f !== required && info.filters.includes(f));
  const field = (filter: ReportFilter) => (
    <FilterField
      key={filter}
      id={`${id}-${filter}`}
      filter={filter}
      value={value.values[filter] || (filter === "grouping" ? defaultGrouping(value.range) : "")}
      onChange={(next) => onChange({ ...value, values: { ...value.values, [filter]: next } })}
    />
  );
  const { range } = value;
  const reversed = isDay(range.from) && isDay(range.to) && range.from > range.to;
  return (
    <div className="flex flex-wrap items-start gap-3">
      {required !== null ? field(required) : null}
      {info.dated ? (
        <DateRangePicker
          value={range}
          onChange={(next) => onChange({ ...value, range: next })}
          error={reversed ? t("validation:rangeOrder") : undefined}
        />
      ) : null}
      {showsAsOf(info) ? (
        <Field id={`${id}-as-of`} label={t("reports:asOf")}>
          <Input
            id={`${id}-as-of`}
            type="date"
            dir="ltr"
            className="w-auto"
            value={value.asOf}
            onChange={(event) => onChange({ ...value, asOf: event.target.value })}
          />
        </Field>
      ) : null}
      {others.map(field)}
    </div>
  );
}

function FilterField({
  id,
  filter,
  value,
  onChange,
}: {
  id: string;
  filter: ReportFilter;
  value: string;
  onChange: (value: string) => void;
}) {
  const { t } = useTranslation();
  switch (filter) {
    case "grouping":
      return (
        <Field id={id} label={t("reports:filters.grouping")}>
          <NativeSelect
            id={id}
            className="w-auto"
            value={value || "day"}
            onChange={(event) => onChange(event.target.value)}
          >
            {GROUPINGS.map((g) => (
              <option key={g} value={g}>
                {t(`reports:filters.groupings.${g}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
      );
    case "terminal":
      return <TerminalSelect id={id} value={value} onChange={onChange} />;
    case "user":
      return <CashierSelect id={id} value={value} onChange={onChange} />;
    case "category":
      return (
        <CategorySelect
          id={id}
          value={value}
          onChange={onChange}
          allLabel={t("reports:filters.allCategories")}
        />
      );
    case "product":
      return <ProductFilter id={id} value={value} onChange={onChange} />;
    case "customer":
      return <CustomerFilter id={id} value={value} onChange={onChange} />;
    case "location":
      return (
        <LocationSelect
          id={id}
          label={t("stock:fields.location")}
          value={value}
          onChange={onChange}
          allLabel={t("stock:fields.allLocations")}
        />
      );
    case "supplier":
      return (
        <SupplierSelect
          id={id}
          value={value}
          onChange={onChange}
          allLabel={t("reports:filters.allSuppliers")}
        />
      );
    case "promotion":
      return <PromotionSelect id={id} value={value} onChange={onChange} />;
    case "count":
      return <CountSelect id={id} value={value} onChange={onChange} />;
  }
}

interface SelectProps {
  id: string;
  value: string;
  onChange: (value: string) => void;
}

/** All terminals or this one (the backend lists no other terminal yet). */
function TerminalSelect({ id, value, onChange }: SelectProps) {
  const { t } = useTranslation();
  const session = useSession().data;
  return (
    <Field id={id} label={t("reports:filters.terminal")}>
      <NativeSelect
        id={id}
        className="w-auto"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="">{t("reports:filters.allTerminals")}</option>
        {session ? (
          <option value={session.terminalId}>
            {t("reports:filters.thisTerminal", { code: session.terminalCode })}
          </option>
        ) : null}
      </NativeSelect>
    </Field>
  );
}

/** The users of the shop, by the names of their login tiles. */
function CashierSelect({ id, value, onChange }: SelectProps) {
  const { t } = useTranslation();
  const users = useUserTiles().data ?? [];
  return (
    <Field id={id} label={t("reports:filters.user")}>
      <NativeSelect
        id={id}
        className="w-auto"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="">{t("reports:filters.allUsers")}</option>
        {users.map((u) => (
          <option key={u.id} value={u.id}>
            {u.displayName}
          </option>
        ))}
      </NativeSelect>
    </Field>
  );
}

/** Every promotion, archived ones too: a declaration is often made once the soldes are over. */
function PromotionSelect({ id, value, onChange }: SelectProps) {
  const { t } = useTranslation();
  const lang = useLang();
  const promotions = usePromotions(true).data ?? [];
  return (
    <Field id={id} label={t("reports:filters.promotion")}>
      <NativeSelect
        id={id}
        className="w-auto"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="">{t("reports:filters.choosePromotion")}</option>
        {promotions.map((p) => (
          <option key={p.id} value={p.id}>
            {localizedName(p, lang)}
          </option>
        ))}
      </NativeSelect>
    </Field>
  );
}

/** The counts in progress and the closed ones, by location and start date. */
function CountSelect({ id, value, onChange }: SelectProps) {
  const { t } = useTranslation();
  const lang = useLang();
  const { nameOf } = useLocations();
  const counts = (useCounts(null).data ?? []).filter((c) => c.status !== "cancelled");
  return (
    <Field id={id} label={t("reports:filters.count")}>
      <NativeSelect
        id={id}
        className="w-auto"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="">{t("reports:filters.chooseCount")}</option>
        {counts.map((c) => (
          <option key={c.id} value={c.id}>
            {t("reports:filters.countOption", {
              location: nameOf(c.locationId),
              date: formatDate(c.startedAt, lang),
              status: t(`stock:counts.status.${c.status}`),
            })}
          </option>
        ))}
      </NativeSelect>
    </Field>
  );
}

/** A product found by name, reference or barcode (archived ones too: past sales name them). */
function ProductFilter({ id, value, onChange }: SelectProps) {
  const { t } = useTranslation();
  const lang = useLang();
  const [text, setText] = useState("");
  const search = useDebounced(text.trim(), 150);
  const hits = useProductSearch(
    { text: search, limit: 8, categoryId: null, includeArchived: true },
    value === "" && search.length >= 2,
  );
  return (
    <SearchPicker
      id={id}
      label={t("reports:filters.product")}
      value={value}
      onChange={onChange}
      text={text}
      onText={setText}
      hits={
        search.length >= 2
          ? (hits.data ?? []).map((h) => ({ id: h.id, name: localizedName(h, lang), detail: h.reference }))
          : []
      }
    />
  );
}

/** A customer found by name, code or phone (the reports' filter; the document archive's too, with
 * its own `label`). */
export function CustomerFilter({ id, value, onChange, label }: SelectProps & { label?: string }) {
  const { t } = useTranslation();
  const [text, setText] = useState("");
  const search = useDebounced(text.trim(), 150);
  const hits = useCustomerSearch(
    { text: search, limit: 8, includeArchived: true },
    value === "" && search.length >= 2,
  );
  return (
    <SearchPicker
      id={id}
      label={label ?? t("reports:filters.customer")}
      value={value}
      onChange={onChange}
      text={text}
      onText={setText}
      hits={
        search.length >= 2 ? (hits.data ?? []).map((h) => ({ id: h.id, name: h.name, detail: h.code })) : []
      }
    />
  );
}

interface Hit {
  id: string;
  name: string;
  detail: string | null;
}

/** All by default; typing a name lists matches under the field, Enter takes the first, and the
 * chosen one shows as a chip that removes the filter. */
function SearchPicker({
  id,
  label,
  value,
  onChange,
  text,
  onText,
  hits,
}: SelectProps & {
  label: string;
  text: string;
  onText: (text: string) => void;
  hits: Hit[];
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const shown = text.trim() === "" ? [] : hits;
  const pick = (hit: Hit) => {
    setName(hit.name);
    onText("");
    onChange(hit.id);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const first = shown[0];
    if (event.key === "Enter" && first) {
      event.preventDefault();
      pick(first);
    } else if (event.key === "Escape") {
      onText("");
    }
  };

  if (value !== "") {
    return (
      <div className="flex flex-col gap-1.5">
        <span id={`${id}-label`} className="font-semibold">
          {label}
        </span>
        <div
          role="group"
          aria-labelledby={`${id}-label`}
          className="inline-flex min-h-11 items-center gap-1 self-start rounded-full bg-brand-soft ps-3 font-semibold"
        >
          <span>{name}</span>
          <button
            type="button"
            aria-label={t("reports:filters.clear", { name })}
            onClick={() => {
              setName("");
              onChange("");
            }}
            className="grid size-11 place-items-center rounded-full text-ink-2 hover:bg-surface focus-visible:outline-3 focus-visible:outline-focus"
          >
            <X aria-hidden className="size-4" />
          </button>
        </div>
      </div>
    );
  }
  return (
    <div className="w-64">
      <Field id={id} label={label} hint={t("reports:filters.searchHint")}>
        {/* the matches drop over the hint, under the field */}
        <div className="relative">
          <Input
            id={id}
            type="search"
            dir="auto"
            autoComplete="off"
            value={text}
            onChange={(event) => onText(event.target.value)}
            onKeyDown={onKeyDown}
          />
          {shown.length > 0 ? (
            <ul
              aria-label={t("reports:filters.results")}
              className="absolute inset-x-0 top-full z-20 mt-1 flex flex-col rounded-control border border-line bg-surface py-1 shadow-[0_12px_32px_rgba(28,25,23,.18)]"
            >
              {shown.map((hit) => (
                <li key={hit.id}>
                  <button
                    type="button"
                    onClick={() => pick(hit)}
                    className="flex min-h-11 w-full items-center gap-2 px-3 text-start hover:bg-brand-soft focus-visible:outline-3 focus-visible:-outline-offset-3 focus-visible:outline-focus"
                  >
                    <span className="truncate">{hit.name}</span>
                    {hit.detail ? (
                      <bdi dir="ltr" className="ms-auto text-sm text-ink-2">
                        {hit.detail}
                      </bdi>
                    ) : null}
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      </Field>
    </div>
  );
}
