import { PowerOff } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { EmptyState } from "@/components/empty-state";
import { Button } from "@/components/ui/button";
import { useSession } from "@/lib/ipc/auth";
import { usePackState } from "@/lib/ipc/packs";
import type { Module } from "@/lib/ipc/types";

/** A pack screen while one of its modules is on; otherwise what to do to turn it on (the owner's
 * way to Settings › Activité et modules). The backend refuses the module's commands anyway. */
export function PackGate({ module, children }: { module: Module | readonly Module[]; children: ReactNode }) {
  const { t } = useTranslation();
  const { data, isPending } = usePackState();
  const { data: session } = useSession();
  if (isPending) return null;
  const wanted: readonly Module[] = typeof module === "string" ? [module] : module;
  if (data && wanted.some((m) => data.modules.includes(m))) return <>{children}</>;
  return (
    <EmptyState
      icon={PowerOff}
      title={t("packs:gate.title")}
      body={t("packs:gate.body")}
      action={
        session?.permissions.includes("settings.company") ? (
          <Button asChild variant="secondary">
            <Link to="/settings/activity">{t("packs:gate.open")}</Link>
          </Button>
        ) : null
      }
    />
  );
}
