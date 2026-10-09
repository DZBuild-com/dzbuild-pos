import { useTranslation } from "react-i18next";
import { type Lang, LANGS } from "@/lib/i18n";
import { cn } from "@/lib/utils";

/** AR / FR / EN as three toggles, each written in its own script (spec §10.5). */
export function LanguageToggle({
  value,
  onChange,
  size = "md",
}: {
  value: Lang;
  onChange: (lang: Lang) => void;
  size?: "md" | "lg";
}) {
  const { t } = useTranslation();
  return (
    <div
      role="radiogroup"
      aria-label={t("common:languages.label")}
      className="inline-flex gap-1 rounded-control border border-line bg-surface p-1"
    >
      {LANGS.map((lang) => (
        <button
          key={lang}
          type="button"
          role="radio"
          aria-checked={value === lang}
          lang={lang}
          onClick={() => onChange(lang)}
          className={cn(
            "rounded-sm px-3 font-semibold",
            size === "lg" ? "min-h-14 px-5 text-lg" : "min-h-11",
            value === lang ? "bg-brand-soft text-ink" : "text-ink-2 hover:bg-surface-2",
          )}
        >
          {t(`common:languages.${lang}`)}
        </button>
      ))}
    </div>
  );
}
