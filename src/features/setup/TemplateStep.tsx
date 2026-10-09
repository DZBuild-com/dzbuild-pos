import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { READY_RELEASES } from "@/features/packs/modules";
import { useTemplates } from "@/lib/ipc/packs";
import type { ShopActivity, TemplateInfo } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { cn } from "@/lib/utils";

/** The shop step's activity (ONB-008): one card per activity template, the ready ones first; the
 * chosen activity goes to `setup_complete_first_run`, which applies its template and seeds. */
export function TemplateStep({
  value,
  onChange,
}: {
  value: ShopActivity;
  onChange: (activity: ShopActivity) => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const templates = useTemplates();
  return (
    <fieldset className="flex flex-col gap-3">
      <legend className="mb-1 font-semibold">{t("setup:shop.activity")}</legend>
      <p className="text-sm text-ink-2">{t("packs:template.pick")}</p>
      {templates.error ? (
        <p role="alert" className="text-danger">
          {errorText(templates.error)}
        </p>
      ) : null}
      {templates.data ? <TemplateCards templates={templates.data} value={value} onChange={onChange} /> : null}
    </fieldset>
  );
}

/** The templates as pressable cards under "Prêts à l'emploi" and "Bientôt enrichis" (a template
 * still growing is chosen the same way: its modules work, its extra seeds arrive later). */
export function TemplateCards({
  templates,
  value,
  onChange,
}: {
  templates: readonly TemplateInfo[];
  value: ShopActivity;
  onChange: (activity: ShopActivity) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  const groups = [
    { key: "ready", items: templates.filter((tpl) => READY_RELEASES.includes(tpl.release)) },
    { key: "growing", items: templates.filter((tpl) => !READY_RELEASES.includes(tpl.release)) },
  ] as const;
  return (
    <div className="flex flex-col gap-4">
      {groups
        .filter((group) => group.items.length > 0)
        .map((group) => (
          <div
            key={group.key}
            role="group"
            aria-labelledby={`${id}-${group.key}`}
            className="flex flex-col gap-2"
          >
            <h3 id={`${id}-${group.key}`} className="font-semibold text-ink-2">
              {t(`packs:template.${group.key}`)}
            </h3>
            <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
              {group.items.map((tpl) => (
                <TemplateCard
                  key={tpl.activity}
                  template={tpl}
                  selected={tpl.activity === value}
                  onSelect={() => onChange(tpl.activity)}
                />
              ))}
            </div>
          </div>
        ))}
    </div>
  );
}

/** A template: its name in the UI language (the card's name), its till screen and its modules
 * (its description). */
function TemplateCard({
  template,
  selected,
  onSelect,
}: {
  template: TemplateInfo;
  selected: boolean;
  onSelect: () => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  return (
    <button
      type="button"
      aria-pressed={selected}
      aria-labelledby={`${id}-name`}
      aria-describedby={`${id}-details`}
      onClick={onSelect}
      className={cn(
        "flex min-h-30 flex-col items-start gap-2 rounded-card border-2 p-3 text-start focus-visible:ring-3 focus-visible:ring-focus focus-visible:outline-none",
        selected ? "border-brand bg-brand-soft" : "border-line bg-surface hover:bg-surface-2",
      )}
    >
      <span id={`${id}-name`} className="font-semibold">
        {t(`packs:activities.${template.activity}`)}
      </span>
      <span id={`${id}-details`} className="flex flex-col gap-1">
        <span className="text-sm text-ink-2">{t(`packs:tillModes.${template.tillMode}.name`)}</span>
        <span className="flex flex-wrap gap-1">
          {template.modules.map((module) => (
            <Badge key={module} className="px-2 text-xs">
              {t(`packs:modules.${module}.name`)}
            </Badge>
          ))}
        </span>
      </span>
    </button>
  );
}
