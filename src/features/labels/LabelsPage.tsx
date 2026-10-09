import { ArrowLeft } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link, useSearchParams } from "react-router";
import { Button } from "@/components/ui/button";
import { useSession } from "@/lib/ipc/auth";
import type { Permission } from "@/lib/ipc/types";
import { cn } from "@/lib/utils";
import { LabelDesigner } from "./LabelDesigner";
import { ScaleItemsTab } from "./ScaleItemsTab";
import { StaleLabelsTab } from "./StaleLabelsTab";

type Tab = "templates" | "stale" | "scale";

/** Who sees each tab: the templates and the reprint queue are `products.labels`; the scale
 * items are read with `products.view` (and edited with `products.edit`). */
const TAB_PERMISSIONS: Record<Tab, Permission> = {
  templates: "products.labels",
  stale: "products.labels",
  scale: "products.view",
};

/** `/products/labels`: label templates, the labels to reprint and the scale items (LBL-001…013,
 * PRD-074). */
export function LabelsPage() {
  const { t } = useTranslation();
  const [params, setParams] = useSearchParams();
  const permissions = useSession().data?.permissions ?? [];
  const tabs = (Object.keys(TAB_PERMISSIONS) as Tab[]).filter((k) =>
    permissions.includes(TAB_PERMISSIONS[k]),
  );
  const requested = params.get("tab");
  const tab = tabs.find((k) => k === requested) ?? tabs[0];

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-2">
        <Button asChild variant="ghost" className="self-start">
          <Link to="/products">
            <ArrowLeft aria-hidden className="rtl:-scale-x-100" />
            {t("products:title")}
          </Link>
        </Button>
        <h1 className="text-3xl font-bold">{t("labels:title")}</h1>
      </div>
      <div role="tablist" className="flex flex-wrap gap-2 border-b border-line pb-2">
        {tabs.map((k) => (
          <button
            key={k}
            type="button"
            role="tab"
            id={`labels-tab-${k}`}
            aria-selected={tab === k}
            aria-controls={`labels-panel-${k}`}
            onClick={() => setParams({ tab: k }, { replace: true })}
            className={cn(
              "min-h-11 rounded-control px-4 font-semibold",
              tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
            )}
          >
            {t(`labels:tabs.${k}`)}
          </button>
        ))}
      </div>
      {tab ? (
        <div role="tabpanel" id={`labels-panel-${tab}`} aria-labelledby={`labels-tab-${tab}`}>
          {tab === "templates" ? <LabelDesigner /> : null}
          {tab === "stale" ? <StaleLabelsTab /> : null}
          {tab === "scale" ? <ScaleItemsTab editable={permissions.includes("products.edit")} /> : null}
        </div>
      ) : null}
    </div>
  );
}
