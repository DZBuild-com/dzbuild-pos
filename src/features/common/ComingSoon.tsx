import { Hourglass } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { EmptyState } from "@/components/empty-state";
import { Button } from "@/components/ui/button";

export function ComingSoon({ labelKey }: { labelKey: string }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  return (
    <EmptyState
      icon={Hourglass}
      title={t("common:comingSoon.title", { module: t(labelKey) })}
      body={t("common:comingSoon.body")}
      action={
        <Button variant="outline" onClick={() => void navigate("/settings")}>
          {t("common:comingSoon.action")}
        </Button>
      }
    />
  );
}
