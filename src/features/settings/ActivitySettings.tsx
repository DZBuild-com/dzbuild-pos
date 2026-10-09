import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { RadioCards } from "@/components/radio-cards";
import { Splash } from "@/components/splash";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { MODULE_GROUPS, TILL_MODES, withModule } from "@/features/packs/modules";
import { usePackState, useResetModules, useSetModules, useSetTillMode } from "@/lib/ipc/packs";
import type { Module } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { ApplyTemplateDialog } from "./ApplyTemplateDialog";

/** Settings › Activité et modules (`settings.company`, ONB-008): the shop's activity and its
 * template, the 21 module switches by group, and the till screen. On a till every change goes to
 * the main PC (an M5 host service); a module turned off hides its screens, its data stays. */
export function ActivitySettings() {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const state = usePackState();
  const setModules = useSetModules();
  const resetModules = useResetModules();
  const setTillMode = useSetTillMode();
  const [changing, setChanging] = useState(false);
  if (state.isPending) return <Splash inline />;
  if (state.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(state.error)}
      </p>
    );
  }
  const current = state.data;
  const saved = () => toast.success(t("packs:saved"));
  const busy = setModules.isPending || resetModules.isPending;
  const failure = setModules.error ?? resetModules.error ?? setTillMode.error;
  return (
    <div className="flex flex-col gap-6">
      <Card aria-labelledby="activity-title">
        <h2 id="activity-title" className="text-xl font-bold">
          {t("packs:settings.activityTitle")}
        </h2>
        <p className="text-ink-2">{t("packs:settings.activityHelp")}</p>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="text-lg font-semibold">{t(`packs:activities.${current.activity}`)}</p>
          <Button variant="secondary" onClick={() => setChanging(true)}>
            {t("packs:template.change")}
          </Button>
        </div>
      </Card>

      <Card aria-labelledby="modules-title">
        <div className="flex flex-wrap items-center gap-3">
          <h2 id="modules-title" className="text-xl font-bold">
            {t("packs:settings.modulesTitle")}
          </h2>
          {current.customised ? (
            <>
              <Badge tone="info">{t("packs:template.customised")}</Badge>
              <Button
                variant="outline"
                className="ms-auto"
                disabled={busy}
                onClick={() => resetModules.mutate(undefined, { onSuccess: saved })}
              >
                {t("packs:template.reset")}
              </Button>
            </>
          ) : null}
        </div>
        <p className="text-ink-2">{t("packs:settings.modulesHelp")}</p>
        <div className="grid gap-6 md:grid-cols-2">
          {MODULE_GROUPS.map(({ group, modules }) => (
            <fieldset key={group} className="flex flex-col gap-2">
              <legend className="mb-1 font-semibold">{t(`packs:groups.${group}`)}</legend>
              {modules.map((module) => (
                <ModuleSwitch
                  key={module}
                  module={module}
                  on={current.modules.includes(module)}
                  disabled={busy}
                  onChange={(on) =>
                    setModules.mutate(withModule(current.modules, module, on), { onSuccess: saved })
                  }
                />
              ))}
            </fieldset>
          ))}
        </div>
      </Card>

      <Card>
        <RadioCards
          legend={t("packs:template.tillMode")}
          name="till-mode"
          value={current.tillMode}
          onChange={(mode) => setTillMode.mutate(mode, { onSuccess: saved })}
          options={TILL_MODES.map((mode) => ({
            value: mode,
            label: t(`packs:tillModes.${mode}.name`),
            hint: t(`packs:tillModes.${mode}.help`),
          }))}
        />
      </Card>

      {failure ? (
        <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
          {errorText(failure)}
        </p>
      ) : null}
      {changing ? (
        <ApplyTemplateDialog
          current={current.activity}
          onClose={() => setChanging(false)}
          onApplied={saved}
        />
      ) : null}
    </div>
  );
}

/** One module: its name is the switch's, its sentence the switch's description. */
function ModuleSwitch({
  module,
  on,
  disabled,
  onChange,
}: {
  module: Module;
  on: boolean;
  disabled: boolean;
  onChange: (on: boolean) => void;
}) {
  const { t } = useTranslation();
  const hint = useId();
  return (
    <div className="flex flex-col gap-0.5">
      <label className="flex min-h-11 items-center gap-3">
        <input
          type="checkbox"
          role="switch"
          aria-describedby={hint}
          className="size-6 accent-brand"
          checked={on}
          disabled={disabled}
          onChange={(event) => onChange(event.target.checked)}
        />
        <span className="font-semibold">{t(`packs:modules.${module}.name`)}</span>
      </label>
      <p id={hint} className="ps-9 text-sm text-ink-2">
        {t(`packs:modules.${module}.help`)}
      </p>
    </div>
  );
}
