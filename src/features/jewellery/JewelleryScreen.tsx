import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router";
import { today } from "@/lib/dates";
import type { Permission, Session } from "@/lib/ipc/types";
import { cn } from "@/lib/utils";
import { JewellerRegister } from "./JewellerRegister";
import { MetalBoard } from "./MetalBoard";

type Tab = "board" | "register";

/** `/jewellery` (VRT-140, VRT-141): the price per gram board and the jeweller register of a year
 * (`?tab=register`, `customers.manage`: it holds the buyers' identity numbers, a manager's read). */
export function JewelleryScreen({ session }: { session: Session }) {
  const { t } = useTranslation();
  const [params, setParams] = useSearchParams();
  const can = (p: Permission) => session.permissions.includes(p);
  const tabs: Tab[] = [
    ...(can("products.view") || can("sales.create") ? (["board"] as const) : []),
    ...(can("customers.manage") ? (["register"] as const) : []),
  ];
  const requested = params.get("tab");
  const tab = tabs.find((k) => k === requested) ?? tabs[0];
  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-3xl font-bold">{t("jewellery:title")}</h1>
      <div
        role="tablist"
        aria-label={t("jewellery:tabs.label")}
        className="flex flex-wrap gap-2 border-b border-line pb-2"
      >
        {tabs.map((k) => (
          <button
            key={k}
            type="button"
            role="tab"
            id={`jewellery-tab-${k}`}
            aria-selected={tab === k}
            aria-controls={`jewellery-panel-${k}`}
            onClick={() => setParams({ tab: k }, { replace: true })}
            className={cn(
              "min-h-11 rounded-control px-4 font-semibold",
              tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
            )}
          >
            {t(`jewellery:tabs.${k}`)}
          </button>
        ))}
      </div>
      {tab ? (
        <div role="tabpanel" id={`jewellery-panel-${tab}`} aria-labelledby={`jewellery-tab-${tab}`}>
          {tab === "board" ? <MetalBoard /> : <JewellerRegister year={Number(today().slice(0, 4))} />}
        </div>
      ) : null}
    </div>
  );
}
