import { PackageOpen } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Route, Routes, useNavigate, useParams, useSearchParams } from "react-router";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { EmptyState } from "@/components/empty-state";
import { NumberSetting, TrilingualSetting } from "@/features/packs/setting-cards";
import { useCan } from "@/features/stock/stock-parts";
import { useUncollected } from "@/lib/ipc/services";
import type { ServiceSummary } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { cn } from "@/lib/utils";
import { ServiceBoard } from "./ServiceBoard";
import { ServiceTicketPage } from "./ServiceTicketPage";

type Tab = "tickets" | "uncollected" | "settings";

/** `/services` (ORD-005/006, RPT-069): the board of tickets and the items not collected; a ticket
 * opens at `/services/<id>`. The tab is kept in `?tab=`. */
export function ServicesScreen() {
  return (
    <Routes>
      <Route index element={<ServicesHome />} />
      <Route path=":id" element={<TicketRoute />} />
    </Routes>
  );
}

function TicketRoute() {
  const { id } = useParams();
  return <ServiceTicketPage key={id} id={id ?? ""} />;
}

function ServicesHome() {
  const { t } = useTranslation();
  const can = useCan();
  const [params, setParams] = useSearchParams();
  // final review: the owner's settings of the repairs (the "ready" message, the conditions)
  const TABS: readonly Tab[] = can("settings.company")
    ? ["tickets", "uncollected", "settings"]
    : ["tickets", "uncollected"];
  const tab = TABS.find((k) => k === params.get("tab")) ?? "tickets";
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-3xl font-bold">{t("services:title")}</h1>
      <div
        role="tablist"
        aria-label={t("services:tabs.label")}
        className="flex flex-wrap gap-2 border-b border-line pb-2"
      >
        {TABS.map((k) => (
          <button
            key={k}
            type="button"
            role="tab"
            id={`services-tab-${k}`}
            aria-selected={tab === k}
            aria-controls={`services-panel-${k}`}
            onClick={() => setParams({ tab: k }, { replace: true })}
            className={cn(
              "min-h-11 rounded-control px-4 font-semibold",
              tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
            )}
          >
            {t(`services:tabs.${k}`)}
          </button>
        ))}
      </div>
      <div role="tabpanel" id={`services-panel-${tab}`} aria-labelledby={`services-tab-${tab}`}>
        {tab === "tickets" ? (
          <ServiceBoard />
        ) : tab === "uncollected" ? (
          <Uncollected />
        ) : (
          <ServicesSettings />
        )}
      </div>
    </div>
  );
}

/** The repairs' settings (final review): the "ready" message, the intake slip's conditions and
 * when a ready item counts as not collected. */
function ServicesSettings() {
  const { t } = useTranslation();
  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <TrilingualSetting
        settingKey="repairs.ready_message"
        id="repairs-ready"
        title={t("packs:settings.repairs.ready")}
        help={t("packs:settings.repairs.readyHelp")}
        max={300}
      />
      <TrilingualSetting
        settingKey="repairs.conditions"
        id="repairs-conditions"
        title={t("packs:settings.repairs.conditions")}
        help={t("packs:settings.repairs.conditionsHelp")}
        max={600}
      />
      <NumberSetting
        settingKey="repairs.uncollected_days"
        id="repairs-uncollected"
        title={t("packs:settings.repairs.uncollected")}
        label={t("packs:settings.repairs.uncollectedDays")}
        help={t("packs:settings.repairs.uncollectedHelp")}
        min={1}
        max={365}
      />
    </div>
  );
}

const col = columnHelper<ServiceSummary>();

/** RPT-069: ready items waiting longer than `repairs.uncollected_days`, with the deposit held. */
function Uncollected() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const errorText = useErrorText();
  const uncollected = useUncollected();
  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("number", {
          header: () => t("services:uncollected.number"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("customerName", { header: () => t("services:uncollected.customer") }),
        col.accessor("item", { header: () => t("services:uncollected.item") }),
        col.accessor((s) => s.readyDays ?? 0, {
          id: "readyDays",
          header: () => t("services:uncollected.readySince"),
          meta: { align: "end" },
          cell: (c) => t("services:waiting", { count: c.getValue() }),
        }),
        col.accessor("deposits", {
          header: () => t("services:uncollected.deposit"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
      ]),
    [t],
  );
  if (uncollected.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(uncollected.error)}
      </p>
    );
  }
  if (uncollected.data?.length === 0) {
    return (
      <EmptyState
        icon={PackageOpen}
        title={t("services:uncollected.emptyTitle")}
        body={t("services:uncollected.emptyBody")}
      />
    );
  }
  return (
    <DataTable
      ariaLabel={t("services:uncollected.label")}
      data={uncollected.data ?? []}
      columns={columns}
      getRowId={(s) => s.id}
      onRowClick={(s) => void navigate(`/services/${s.id}`)}
      empty={t("common:status.loading")}
    />
  );
}
