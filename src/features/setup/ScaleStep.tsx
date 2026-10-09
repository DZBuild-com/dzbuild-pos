import { Scale } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ScaleCard } from "@/features/settings/hardware/ScaleCard";
import type { HardwareConfig } from "@/lib/ipc/types";

/** Wizard step 5 (ONB-009), for a supérette, a butcher or a bakery: M4's scale card (port,
 * protocol, detection), kept in the wizard's draft of hardware.json and saved when it ends; the
 * owner may skip it. */
export function ScaleStep({
  config,
  onChange,
}: {
  config: HardwareConfig;
  onChange: (config: HardwareConfig) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-3">
        <Scale aria-hidden className="size-10 shrink-0 text-ink-2" />
        <div>
          <h2 className="text-2xl font-bold">{t("packs:scale.title")}</h2>
          <p className="text-ink-2">{t("packs:scale.help")}</p>
        </div>
      </div>
      <ScaleCard config={config} onChange={onChange} />
    </div>
  );
}
