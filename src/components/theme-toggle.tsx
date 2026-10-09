import { Monitor, Moon, Sun } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { nextTheme, type ThemePreference } from "@/lib/theme";

const ICONS = { system: Monitor, light: Sun, dark: Moon } as const;

export function ThemeToggle({
  value,
  onChange,
}: {
  value: ThemePreference;
  onChange: (value: ThemePreference) => void;
}) {
  const { t } = useTranslation();
  const Icon = ICONS[value];
  return (
    <Button
      variant="ghost"
      size="icon"
      aria-label={t(`common:theme.${value}`)}
      title={t("common:theme.label")}
      onClick={() => onChange(nextTheme(value))}
    >
      <Icon aria-hidden />
    </Button>
  );
}
