import { useTranslation } from "react-i18next";
import { markUrl } from "@/lib/brand";
import { cn } from "@/lib/utils";

export function Splash({ inline = false }: { inline?: boolean }) {
  const { t } = useTranslation();
  return (
    <div role="status" className={cn("grid place-items-center", inline ? "min-h-40" : "min-h-dvh bg-bg")}>
      <img src={markUrl} alt="" className="size-16 motion-safe:animate-pulse" />
      <span className="sr-only">{t("common:status.loading")}</span>
    </div>
  );
}
