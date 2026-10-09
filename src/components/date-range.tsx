import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { type DateRange, RANGE_PRESETS, type RangePreset, presetOf, rangeOf, today } from "@/lib/dates";

/**
 * A period: a preset (today, this week, last month…; weeks start on Sunday) and the two days it
 * covers, which can also be typed. Without presets it is just the two days (`presets={[]}`).
 * `error` shows under the end day.
 */
export function DateRangePicker({
  value,
  onChange,
  presets = RANGE_PRESETS,
  error,
}: {
  value: DateRange;
  onChange: (range: DateRange) => void;
  presets?: readonly RangePreset[];
  error?: string;
}) {
  const { t } = useTranslation();
  const id = useId();
  const [custom, setCustom] = useState(false);
  const day = today();
  const preset = custom ? "custom" : presetOf(value, day, presets);
  return (
    <div className="flex flex-wrap items-start gap-3">
      {presets.length > 0 ? (
        <Field id={`${id}-preset`} label={t("tables:range.label")}>
          <NativeSelect
            id={`${id}-preset`}
            className="w-auto"
            value={preset}
            onChange={(event) => {
              const chosen = presets.find((p) => p === event.target.value);
              if (!chosen) return;
              setCustom(chosen === "custom");
              if (chosen !== "custom") onChange(rangeOf(chosen, day));
            }}
          >
            {presets.map((p) => (
              <option key={p} value={p}>
                {t(`tables:range.${p}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
      ) : null}
      <Field id={`${id}-from`} label={t("tables:range.from")}>
        <Input
          id={`${id}-from`}
          type="date"
          dir="ltr"
          className="w-auto"
          value={value.from}
          max={value.to || undefined}
          onChange={(event) => onChange({ ...value, from: event.target.value })}
        />
      </Field>
      <Field id={`${id}-to`} label={t("tables:range.to")} error={error}>
        <Input
          id={`${id}-to`}
          type="date"
          dir="ltr"
          className="w-auto"
          aria-invalid={Boolean(error)}
          value={value.to}
          min={value.from || undefined}
          onChange={(event) => onChange({ ...value, to: event.target.value })}
        />
      </Field>
    </div>
  );
}
