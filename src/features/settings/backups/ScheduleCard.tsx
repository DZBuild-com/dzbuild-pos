import { useTranslation } from "react-i18next";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { useTillLink } from "@/lib/ipc/lan";
import { type SettingKey, useSetSetting, useSetting } from "@/lib/ipc/settings";
import { useErrorText } from "@/lib/use-error-text";

/** The retention choices (DAT-007), inside the ranges pos-core's settings registry accepts. */
const CHOICES: Record<Extract<SettingKey, `backup.keep_${string}`>, number[]> = {
  "backup.keep_daily": [7, 14, 30, 60],
  "backup.keep_weekly": [0, 4, 8, 12, 26, 52],
  "backup.keep_monthly": [0, 6, 12, 24, 36],
  // DAT-007 hourly part (Task 17): the copies of the day kept while a till is open, 0 = none
  "backup.keep_hourly": [0, 6, 12, 24, 48],
};

function Keep({
  settingKey,
  label,
  unit,
  set,
  fallback,
}: {
  settingKey: keyof typeof CHOICES;
  label: string;
  unit: "days" | "weeks" | "months" | "copies";
  set: ReturnType<typeof useSetSetting>;
  /** Shown until the setting answers (the backend's default). */
  fallback?: number;
}) {
  const { t } = useTranslation();
  const value = useSetting(settingKey);
  const id = settingKey.replace(".", "-");
  return (
    <Field id={id} label={label}>
      <NativeSelect
        id={id}
        value={String(value.data ?? fallback ?? "")}
        onChange={(event) => set.mutate({ key: settingKey, value: Number(event.target.value) })}
      >
        {CHOICES[settingKey].map((n) => (
          <option key={n} value={n}>
            {n === 0
              ? t("settings:backups.schedule.none")
              : unit === "copies"
                ? String(n)
                : t(`settings:backups.schedule.${unit}`, { count: n })}
          </option>
        ))}
      </NativeSelect>
    </Field>
  );
}

/** What is backed up by itself, the backup when the app closes (DAT-024) and how long the
 * automatic copies are kept (DAT-007, DAT-022). Shared settings, the main PC's; read-only
 * on a till (its own replica follows the main PC's schedule). */
export function ScheduleCard() {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const onClose = useSetting("backup.on_close");
  const set = useSetSetting();
  const till = useTillLink();
  return (
    <Card aria-labelledby="backup-schedule-title">
      <h2 id="backup-schedule-title" className="text-xl font-bold">
        {t("settings:backups.schedule.title")}
      </h2>
      <ul className="list-disc ps-6 text-ink-2">
        <li>{t("settings:backups.schedule.daily")}</li>
        <li>{t("settings:backups.schedule.zClose")}</li>
        <li>{t("settings:backups.schedule.beforeUpdate")}</li>
      </ul>
      <HostOnlyNote />
      <fieldset disabled={till.isTill} className="contents">
        <label className="flex min-h-11 items-center gap-3">
          <input
            type="checkbox"
            role="switch"
            className="size-6 accent-brand"
            checked={onClose.data === true}
            onChange={(event) => set.mutate({ key: "backup.on_close", value: event.target.checked })}
          />
          <span>{t("settings:backups.schedule.onClose")}</span>
        </label>
        <div className="grid gap-4 sm:grid-cols-3">
          <Keep
            settingKey="backup.keep_daily"
            label={t("settings:backups.schedule.keepDaily")}
            unit="days"
            set={set}
          />
          <Keep
            settingKey="backup.keep_weekly"
            label={t("settings:backups.schedule.keepWeekly")}
            unit="weeks"
            set={set}
          />
          <Keep
            settingKey="backup.keep_monthly"
            label={t("settings:backups.schedule.keepMonthly")}
            unit="months"
            set={set}
          />
        </div>
        <div className="flex flex-col gap-1 sm:max-w-sm">
          <Keep
            settingKey="backup.keep_hourly"
            label={t("config:backups.hourly.label")}
            unit="copies"
            set={set}
            fallback={24}
          />
          <p className="text-sm text-ink-2">{t("config:backups.hourly.hint")}</p>
        </div>
      </fieldset>
      {set.error ? (
        <p role="alert" className="text-danger">
          {errorText(set.error)}
        </p>
      ) : null}
      <p className="text-sm text-ink-2">{t("settings:backups.schedule.retentionNote")}</p>
    </Card>
  );
}
