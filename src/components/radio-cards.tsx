import { useId } from "react";
import { cn } from "@/lib/utils";

/** Big radio choices. A choice's name is its label; its hint is its description (a screen reader
 * says both, a test finds the choice by its label alone). */
export function RadioCards<T extends string>({
  legend,
  name,
  value,
  onChange,
  options,
}: {
  legend: string;
  name: string;
  value: T;
  onChange: (value: T) => void;
  options: { value: T; label: string; hint?: string }[];
}) {
  const id = useId();
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="mb-1 font-semibold">{legend}</legend>
      <div className="grid gap-3 sm:grid-cols-2">
        {options.map((option, index) => {
          const labelId = `${id}-${index}`;
          const hintId = `${id}-${index}-hint`;
          return (
            <label
              key={option.value}
              className={cn(
                "flex cursor-pointer flex-col gap-1 rounded-card border-2 p-4",
                value === option.value ? "border-brand bg-brand-soft" : "border-line bg-surface",
              )}
            >
              <span className="flex items-center gap-2 font-semibold">
                <input
                  type="radio"
                  name={name}
                  value={option.value}
                  checked={value === option.value}
                  onChange={() => onChange(option.value)}
                  aria-labelledby={option.hint ? labelId : undefined}
                  aria-describedby={option.hint ? hintId : undefined}
                  className="size-5 accent-brand"
                />
                <span id={labelId}>{option.label}</span>
              </span>
              {option.hint ? (
                <span id={hintId} className="text-sm text-ink-2">
                  {option.hint}
                </span>
              ) : null}
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}
