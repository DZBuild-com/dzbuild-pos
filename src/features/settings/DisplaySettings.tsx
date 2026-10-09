import { useTranslation } from "react-i18next";
import { RadioCards } from "@/components/radio-cards";
import { Splash } from "@/components/splash";
import { Card } from "@/components/ui/card";
import { useDisplay, useSetDisplay } from "@/lib/ipc/display";
import type { DisplayPrefs, TextSize } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";

const TEXT_SIZES: readonly TextSize[] = ["normal", "large", "larger"];

/** Réglages › Affichage (UX-009): light mode for a slow PC, the text size and the on-screen
 * keyboard. This PC's choices (`local.json`), applied to the whole window at once. */
export function DisplaySettings() {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const display = useDisplay();
  const set = useSetDisplay();
  if (display.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(display.error)}
      </p>
    );
  }
  const current = display.data;
  if (!current) return <Splash inline />;
  const change = (patch: Partial<DisplayPrefs>) => set.mutate({ ...current, ...patch });
  return (
    <Card aria-labelledby="display-title">
      <h2 id="display-title" className="text-xl font-bold">
        {t("settings:sections.display")}
      </h2>
      <div className="flex flex-col gap-1">
        <label className="flex min-h-11 items-center gap-3">
          <input
            type="checkbox"
            role="switch"
            aria-describedby="display-light-hint"
            className="size-6 accent-brand"
            checked={current.lightMode}
            onChange={(event) => change({ lightMode: event.target.checked })}
          />
          <span>{t("settings:display.light")}</span>
        </label>
        <p id="display-light-hint" className="ps-9 text-sm text-ink-2">
          {t("settings:display.lightHint")}
        </p>
      </div>
      <RadioCards<TextSize>
        legend={t("settings:display.textSize")}
        name="display-text-size"
        value={current.textSize}
        onChange={(textSize) => change({ textSize })}
        options={TEXT_SIZES.map((size) => ({ value: size, label: t(`settings:display.${size}`) }))}
      />
      <label className="flex min-h-11 items-center gap-3">
        <input
          type="checkbox"
          role="switch"
          className="size-6 accent-brand"
          checked={current.onscreenKeyboard}
          onChange={(event) => change({ onscreenKeyboard: event.target.checked })}
        />
        <span>{t("settings:display.osk")}</span>
      </label>
      <p className="text-sm text-ink-2">{t("settings:display.thisPc")}</p>
      {set.error ? (
        <p role="alert" className="text-danger">
          {errorText(set.error)}
        </p>
      ) : null}
    </Card>
  );
}
