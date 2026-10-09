import { type ReactNode, Suspense } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router";
import { ImportLink } from "@/features/imports/import-entities";
import { WasteScreen, YieldScreen } from "@/features/packs/pack-screens";
import { usePackState } from "@/lib/ipc/pack-state";
import { cn } from "@/lib/utils";
import { CountsTab } from "./CountsTab";
import { ExpiryTab } from "./ExpiryTab";
import { LevelsTab } from "./LevelsTab";
import { MovementsTab } from "./MovementsTab";
import { SerialsTab } from "./SerialsTab";
import { useCan } from "./stock-parts";
import { TransfersTab } from "./TransferDialog";

type Tab = "levels" | "movements" | "counts" | "transfers" | "expiry" | "serials" | "yield" | "waste";

const PANELS: Record<Tab, () => ReactNode> = {
  levels: () => <LevelsTab />,
  movements: () => <MovementsTab />,
  counts: () => <CountsTab />,
  transfers: () => <TransfersTab />,
  expiry: () => <ExpiryTab />,
  serials: () => <SerialsTab />,
  // M8 butcher: each a chunk of its own, shown while its module is on
  yield: () => <YieldScreen />,
  waste: () => <WasteScreen />,
};

/** `/stock`: levels per location, the movement log, counts, transfers, expiry and serial numbers
 * (STK-002…015, PRD-050/052), and the butcher's cutting and day's waste while their modules are
 * on (VRT-011, STK-026). A counter holding only `stock.count` sees the counts alone; the tab is
 * kept in `?tab=`. */
export function StockScreen() {
  const { t } = useTranslation();
  const can = useCan();
  const [params, setParams] = useSearchParams();
  const modules = usePackState().data?.modules ?? [];
  const tabs: Tab[] = [
    ...(can("stock.view") ? (["levels", "movements"] as const) : []),
    ...(can("stock.view") || can("stock.count") ? (["counts"] as const) : []),
    ...(can("stock.view") ? (["transfers", "expiry", "serials"] as const) : []),
    ...(can("stock.view") && modules.includes("yield") ? (["yield"] as const) : []),
    ...(can("stock.view") && modules.includes("waste") ? (["waste"] as const) : []),
  ];
  const requested = params.get("tab");
  const tab = tabs.find((k) => k === requested) ?? tabs[0];
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <h1 className="text-3xl font-bold">{t("stock:title")}</h1>
        <ImportLink entities={["opening_stock", "count_lines"]} />
      </div>
      <div
        role="tablist"
        aria-label={t("stock:tabs.label")}
        className="flex flex-wrap gap-2 border-b border-line pb-2"
      >
        {tabs.map((k) => (
          <button
            key={k}
            type="button"
            role="tab"
            id={`stock-tab-${k}`}
            aria-selected={tab === k}
            aria-controls={`stock-panel-${k}`}
            onClick={() => setParams({ tab: k }, { replace: true })}
            className={cn(
              "min-h-11 rounded-control px-4 font-semibold",
              tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
            )}
          >
            {k === "yield" || k === "waste" ? t(`butcher:tabs.${k}`) : t(`stock:tabs.${k}`)}
          </button>
        ))}
      </div>
      {tab ? (
        <div role="tabpanel" id={`stock-panel-${tab}`} aria-labelledby={`stock-tab-${tab}`}>
          <Suspense fallback={null}>{PANELS[tab]()}</Suspense>
        </div>
      ) : null}
    </div>
  );
}
