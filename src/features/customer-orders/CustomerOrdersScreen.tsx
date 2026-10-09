import { ChevronLeft, ChevronRight } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Route, Routes, useParams, useSearchParams } from "react-router";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { isDay, today } from "@/lib/dates";
import { ProductSetting } from "@/features/packs/setting-cards";
import { useCan } from "@/features/stock/stock-parts";
import { useModule } from "@/lib/ipc/pack-state";
import type { CustomerOrderStatus, OrderKind } from "@/lib/ipc/types";
import { normalizeDigits } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { cn } from "@/lib/utils";
import { AppointmentBook } from "./AppointmentBook";
import { OrderPage } from "./OrderPage";
import { OrdersTable } from "./OrdersTable";
import { shiftDay } from "./slots";

type Tab = "orders" | "appointments";
const STATUSES: readonly CustomerOrderStatus[] = ["open", "ready", "fulfilled", "closed"];
const KINDS: readonly OrderKind[] = ["order", "layaway"];

/** `/customer-orders` (ORD-002/003/009; `/orders` is the DZBuild store's online orders): the
 * customer orders and layaways, the appointment book (`?tab=`), an order at `/customer-orders/<id>`. */
export function CustomerOrdersScreen() {
  return (
    <Routes>
      <Route index element={<OrdersHome />} />
      <Route path=":id" element={<OrderRoute />} />
    </Routes>
  );
}

function OrderRoute() {
  const { id } = useParams();
  return <OrderPage key={id} id={id ?? ""} />;
}

function OrdersHome() {
  const { t } = useTranslation();
  const ordersOn = useModule("orders");
  const appointmentsOn = useModule("appointments");
  // A tab per module that is on; both while the modules cannot be read (the backend still refuses
  // the writes of a module that is off).
  const tabs: Tab[] =
    ordersOn || appointmentsOn
      ? [...(ordersOn ? (["orders"] as const) : []), ...(appointmentsOn ? (["appointments"] as const) : [])]
      : ["orders", "appointments"];
  const [params, setParams] = useSearchParams();
  const tab = tabs.find((k) => k === params.get("tab")) ?? tabs[0] ?? "orders";
  const label = (k: Tab) => t(k === "orders" ? "orders:tabs.orders" : "orders:appointments.title");
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-3xl font-bold">{t("orders:title")}</h1>
      {tabs.length > 1 ? (
        <div
          role="tablist"
          aria-label={t("orders:tabs.label")}
          className="flex flex-wrap gap-2 border-b border-line pb-2"
        >
          {tabs.map((k) => (
            <button
              key={k}
              type="button"
              role="tab"
              id={`orders-tab-${k}`}
              aria-selected={tab === k}
              aria-controls={`orders-panel-${k}`}
              onClick={() => setParams({ tab: k }, { replace: true })}
              className={cn(
                "min-h-11 rounded-control px-4 font-semibold",
                tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
              )}
            >
              {label(k)}
            </button>
          ))}
        </div>
      ) : null}
      {tabs.length > 1 ? (
        <div role="tabpanel" id={`orders-panel-${tab}`} aria-labelledby={`orders-tab-${tab}`}>
          {tab === "orders" ? <Orders /> : <Appointments />}
        </div>
      ) : tab === "orders" ? (
        <Orders />
      ) : (
        <Appointments />
      )}
    </div>
  );
}

/** The product the kept deposits of a cancelled order are sold as (final review). */
function KeptDepositSetting() {
  const { t } = useTranslation();
  return (
    <ProductSetting
      settingKey="orders.kept_deposit_product_id"
      title={t("packs:settings.keptDeposit.title")}
      help={t("packs:settings.keptDeposit.help")}
      none={t("packs:settings.keptDeposit.none")}
      clear={t("packs:settings.keptDeposit.clear")}
    />
  );
}

/** The orders by status, kind and a text (number or customer). */
function Orders() {
  const { t } = useTranslation();
  const can = useCan();
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState<CustomerOrderStatus | null>(null);
  const [kind, setKind] = useState<OrderKind | null>(null);
  const text = useDebounced(normalizeDigits(search.trim()), 200);
  return (
    <div className="flex flex-col gap-4">
      <TableToolbar
        search={search}
        onSearch={setSearch}
        searchHint={t("orders:list.searchHint")}
        exportTarget={null}
        filters={
          <>
            <Field id="orders-status" label={t("orders:list.status")}>
              <NativeSelect
                id="orders-status"
                className="w-auto"
                value={status ?? ""}
                onChange={(e) => setStatus(STATUSES.find((s) => s === e.target.value) ?? null)}
              >
                <option value="">{t("orders:list.allStatuses")}</option>
                {STATUSES.map((s) => (
                  <option key={s} value={s}>
                    {t(`orders:status.${s}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            <Field id="orders-kind" label={t("orders:list.kind")}>
              <NativeSelect
                id="orders-kind"
                className="w-auto"
                value={kind ?? ""}
                onChange={(e) => setKind(KINDS.find((k) => k === e.target.value) ?? null)}
              >
                <option value="">{t("orders:list.allKinds")}</option>
                {KINDS.map((k) => (
                  <option key={k} value={k}>
                    {t(`orders:kinds.${k}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          </>
        }
      />
      <OrdersTable filter={{ text, status, kind }} />
      <p className="text-sm text-ink-2">{t("orders:list.hint")}</p>
      {can("settings.company") ? <KeptDepositSetting /> : null}
    </div>
  );
}

/** The appointment book of a day, today first. */
function Appointments() {
  const { t } = useTranslation();
  const [day, setDay] = useState(today());
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end gap-2">
        <Button
          variant="outline"
          size="icon"
          aria-label={t("orders:appointments.previousDay")}
          title={t("orders:appointments.previousDay")}
          onClick={() => setDay((d) => shiftDay(d, -1))}
        >
          <ChevronLeft aria-hidden className="rtl:-scale-x-100" />
        </Button>
        <Field id="appointments-day" label={t("orders:appointments.day")}>
          <Input
            id="appointments-day"
            type="date"
            dir="ltr"
            className="w-auto"
            value={day}
            onChange={(e) => {
              if (isDay(e.target.value)) setDay(e.target.value);
            }}
          />
        </Field>
        <Button
          variant="outline"
          size="icon"
          aria-label={t("orders:appointments.nextDay")}
          title={t("orders:appointments.nextDay")}
          onClick={() => setDay((d) => shiftDay(d, 1))}
        >
          <ChevronRight aria-hidden className="rtl:-scale-x-100" />
        </Button>
        <Button variant="secondary" onClick={() => setDay(today())}>
          {t("orders:appointments.today")}
        </Button>
      </div>
      <AppointmentBook key={day} day={day} />
    </div>
  );
}
