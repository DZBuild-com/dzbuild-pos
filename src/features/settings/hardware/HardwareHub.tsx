import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Splash } from "@/components/splash";
import { Button } from "@/components/ui/button";
import { toAppError } from "@/lib/ipc/client";
import { useFoundDevices, useHardwareConfig, useSaveHardware } from "@/lib/ipc/hardware";
import { useModule } from "@/lib/ipc/pack-state";
import type { HardwareConfig } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { cn } from "@/lib/utils";
import { HardwareSettings } from "../HardwareSettings";
import { DisplayCard } from "./DisplayCard";
import { DoctorPanel } from "./DoctorPanel";
import { DrawerCard } from "./DrawerCard";
import { KitchenPrintersCard } from "./KitchenPrintersCard";
import { LabelPrintersCard } from "./LabelPrintersCard";
import { ScaleCard } from "./ScaleCard";
import { ScaleCodesCard } from "./ScaleCodesCard";
import { ScannersCard } from "./ScannersCard";

const TABS = ["receipt", "labels", "scale", "displays", "scanners", "drawer", "kitchen"] as const;
type Tab = (typeof TABS)[number];

/** The tab that holds a refused field, by the first segment of its path (`display.vfd.…`). */
export function tabOf(field: string): Tab {
  const section = field.split(".")[0] ?? "";
  if (section === "labelPrinters" || section === "labelPrinterId") return "labels";
  if (section === "scale") return "scale";
  if (section === "display") return "displays";
  if (section === "scanner") return "scanners";
  if (section === "drawer") return "drawer";
  if (section === "kitchen") return "kitchen";
  return "receipt";
}

/** What the hub checks before saving, as `hardware_save` would refuse it: a ticked serial device
 * without its port, a pole display's columns, the customer screen's thanks. The field's path and
 * the words that say what to do. */
export function checkHardware(config: HardwareConfig): { field: string; key: string } | null {
  const blank = (port: string | undefined) => (port ?? "").trim() === "";
  const portRequired = "hardware:hub.portRequired";
  if (config.scale && blank(config.scale.serial.port))
    return { field: "scale.serial.port", key: portRequired };
  const vfd = config.display?.vfd;
  if (vfd && blank(vfd.serial.port)) return { field: "display.vfd.serial.port", key: portRequired };
  if (vfd && (vfd.columns < 16 || vfd.columns > 40)) {
    return { field: "display.vfd.columns", key: "hardware:hub.columnsRange" };
  }
  const thanks = config.display?.screen?.thanksSeconds;
  if (thanks !== undefined && (thanks < 2 || thanks > 30)) {
    return { field: "display.screen.thanksSeconds", key: "hardware:hub.thanksRange" };
  }
  const serialScanner = config.scanner?.serial;
  if (serialScanner && blank(serialScanner.serial.port)) {
    return { field: "scanner.serial.serial.port", key: portRequired };
  }
  if (config.drawer?.kind === "serial" && blank(config.drawer.port))
    return { field: "drawer.port", key: portRequired };
  return null;
}

export interface CardProps {
  config: HardwareConfig;
  onChange: (config: HardwareConfig) => void;
  /** What stops the save at the field `path` (`display.vfd.serial.port`…): the hub's own check or
   * `hardware_save`'s refusal, shown on that field. */
  errorFor?: (path: string) => string | undefined;
}

/** Réglages › Matériel (settings.hardware, ONB-009): every device of this PC, one draft, one save.
 * Nothing reaches `hardware.json` before "Enregistrer les réglages du matériel". */
export function HardwareHub() {
  const errorText = useErrorText();
  const loaded = useHardwareConfig();
  // Windows printers and COM ports, listed as the hub opens: the tabs choose from them.
  useFoundDevices();
  if (loaded.isPending) return <Splash inline />;
  if (loaded.isError) {
    return (
      <p role="alert" className="text-danger">
        {errorText(loaded.error)}
      </p>
    );
  }
  return <Hub saved={loaded.data} />;
}

function Hub({ saved }: { saved: HardwareConfig }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const save = useSaveHardware();
  const [tab, setTab] = useState<Tab>("receipt");
  // RST-006, HW-004: the kitchen printers per preparation station, in a restaurant
  const restaurant = useModule("restaurant");
  const tabs = TABS.filter((id) => id !== "kitchen" || restaurant);
  // Loaded once: the tabs edit this draft, which only `hardware_save` writes back.
  const [config, setConfig] = useState<HardwareConfig>(saved);
  // The hub's own check before saving: the field and what to do.
  const [problem, setProblem] = useState<{ field: string; text: string } | null>(null);
  const refused = save.error ? toAppError(save.error) : null;
  const errorFor = (path: string) =>
    problem?.field === path
      ? problem.text
      : refused?.field === path && save.error
        ? errorText(save.error)
        : undefined;
  const card = { config, onChange: setConfig, errorFor };
  const submit = () => {
    save.reset();
    const found = checkHardware(config);
    if (found) {
      setProblem({ field: found.field, text: t(found.key) });
      setTab(tabOf(found.field));
      return;
    }
    setProblem(null);
    save.mutate(config, {
      // the refused field is shown on its tab
      onError: (error) => {
        const field = toAppError(error).field;
        if (field) setTab(tabOf(field));
      },
    });
  };
  return (
    <div className="flex flex-col gap-6">
      <DoctorPanel config={saved} />
      <div
        role="tablist"
        aria-label={t("hardware:hub.title")}
        className="flex flex-wrap gap-2 border-b border-line"
      >
        {tabs.map((id) => (
          <button
            key={id}
            id={`hardware-tab-${id}`}
            type="button"
            role="tab"
            aria-selected={tab === id}
            aria-controls="hardware-panel"
            className={cn(
              "min-h-11 rounded-t-control px-4 font-semibold",
              tab === id ? "bg-brand-soft text-ink" : "text-ink-2",
            )}
            onClick={() => setTab(id)}
          >
            {id === "kitchen" ? t("config:hardware.kitchenTab") : t(`hardware:hub.tabs.${id}`)}
          </button>
        ))}
      </div>
      <div
        id="hardware-panel"
        role="tabpanel"
        aria-labelledby={`hardware-tab-${tab}`}
        className="flex flex-col gap-6"
      >
        {tab === "receipt" ? <HardwareSettings config={config} onChange={setConfig} embedded /> : null}
        {tab === "labels" ? <LabelPrintersCard {...card} /> : null}
        {tab === "scale" ? (
          <>
            <ScaleCard {...card} />
            <ScaleCodesCard />
          </>
        ) : null}
        {tab === "displays" ? <DisplayCard {...card} /> : null}
        {tab === "scanners" ? <ScannersCard {...card} /> : null}
        {tab === "drawer" ? <DrawerCard {...card} /> : null}
        {tab === "kitchen" ? <KitchenPrintersCard {...card} /> : null}
      </div>
      <div className="flex flex-col gap-2">
        <div>
          <Button variant="primary" disabled={save.isPending} onClick={submit}>
            {t("hardware:save")}
          </Button>
        </div>
        {save.isSuccess ? (
          <p role="status" className="text-ok">
            {t("hardware:saved")}
          </p>
        ) : null}
        {problem ? (
          <p role="alert" className="text-danger">
            {t("hardware:hub.fixField")}
          </p>
        ) : null}
        {save.error ? (
          <p role="alert" className="text-danger">
            {errorText(save.error)}
          </p>
        ) : null}
      </div>
    </div>
  );
}
