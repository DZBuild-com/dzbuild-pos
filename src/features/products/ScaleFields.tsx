import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Field } from "@/components/ui/field";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { toAppError } from "@/lib/ipc/client";
import type { ScaleItem, ScaleItemInput } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";

/** The scale settings of a product as typed: empty is "none" for the key and the shelf life. */
export interface ScaleDraft {
  hotkey: string;
  tareG: string;
  shelfLifeDays: string;
}

export const SCALE_FIELDS = [
  "hotkey",
  "tareG",
  "shelfLifeDays",
] as const satisfies readonly (keyof ScaleItemInput)[];

const LABELS: Record<keyof ScaleDraft, string> = {
  hotkey: "products:scale.hotkey",
  tareG: "products:scale.tare",
  shelfLifeDays: "products:scale.shelfLife",
};

/** What the scale knows of a product, or nothing yet (0 g of tare). */
export function scaleDraft(item: ScaleItem | undefined): ScaleDraft {
  return {
    hotkey: item?.hotkey === null || item?.hotkey === undefined ? "" : String(item.hotkey),
    tareG: String(item?.tareG ?? 0),
    shelfLifeDays:
      item?.shelfLifeDays === null || item?.shelfLifeDays === undefined ? "" : String(item.shelfLifeDays),
  };
}

/** The draft as `scale_items_set` takes it; Rust checks the ranges and a key taken twice. */
export function scaleInput(draft: ScaleDraft): ScaleItemInput {
  const optional = (text: string) => (text.trim() === "" ? null : Number(text));
  return {
    hotkey: optional(draft.hotkey),
    tareG: draft.tareG.trim() === "" ? 0 : Number(draft.tareG),
    shelfLifeDays: optional(draft.shelfLifeDays),
  };
}

/** PRD-074 under the PLU: the scale key, the tare and the shelf life, saved with the product. A
 * refusal of `scale_items_set` shows under the field it names. `readOnly` on a till: the scale
 * items are the main PC's. */
export function ScaleFields({
  value,
  onChange,
  error,
  readOnly = false,
}: {
  value: ScaleDraft;
  onChange: (value: ScaleDraft) => void;
  error: unknown;
  readOnly?: boolean;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const failed = error ? toAppError(error) : null;
  return (
    <fieldset className="grid gap-4 sm:col-span-2 sm:grid-cols-3">
      <legend className="mb-3 font-semibold">{t("products:scale.title")}</legend>
      {readOnly ? <HostOnlyNote className="sm:col-span-3" /> : null}
      {SCALE_FIELDS.map((field) => (
        <Field
          key={field}
          id={`p-scale-${field}`}
          label={t(LABELS[field])}
          error={failed?.field === field ? errorText(failed) : undefined}
        >
          <NumericInput
            id={`p-scale-${field}`}
            kind="integer"
            disabled={readOnly}
            value={value[field]}
            onValueChange={(text) => onChange({ ...value, [field]: text })}
          />
        </Field>
      ))}
    </fieldset>
  );
}
