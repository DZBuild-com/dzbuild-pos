import { useTranslation } from "react-i18next";
import { RadioCards } from "@/components/radio-cards";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { LocationSelect, useLocations } from "@/features/stock/stock-parts";
import { formatDate, formatNumber } from "@/lib/format";
import { useCounts } from "@/lib/ipc/stock-m2";
import type { FieldInfo, ImportOptions, ImportPolicy, ImportPreview } from "@/lib/ipc/types";
import { useLang } from "@/lib/use-language";
import { POSTING_IMPORTS } from "./import-entities";

/** First values of a column shown under its title. */
const SAMPLES = 3;
/** The header row can be moved down to this row at most. */
const HEADER_ROWS = 20;

/** A field's label in the select: required ones starred. */
export function useFieldLabel(): (field: FieldInfo) => string {
  const { t } = useTranslation();
  return (field) => (field.required ? t("imports:mapping.required", { label: field.label }) : field.label);
}

/** The required fields no column feeds yet. */
export function missingFields(preview: ImportPreview, options: ImportOptions): FieldInfo[] {
  return preview.fields.filter((f) => f.required && !options.mapping.includes(f.key));
}

/**
 * IMX-006, step 2: what each column of the file feeds (the backend's suggestion first, "Ignorer"
 * leaves it out), the title row, what to do with existing records, and where opening stock or
 * counted quantities go. A field goes to one column only: choosing it again moves it. Another
 * title row is read again by the backend (`onHeaderRow`): its titles, samples and suggestion.
 */
export function MappingStep({
  preview,
  options,
  onChange,
  onHeaderRow,
}: {
  preview: ImportPreview;
  options: ImportOptions;
  onChange: (options: ImportOptions) => void;
  onHeaderRow: (row: number) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const fieldLabel = useFieldLabel();
  const entity = options.entity;
  const policies: ImportPolicy[] =
    entity === "products" ? ["create_only", "upsert", "sync"] : ["create_only", "upsert"];
  const setField = (column: number, key: string | null) =>
    onChange({
      ...options,
      mapping: options.mapping.map((current, i) => (i === column ? key : current === key ? null : current)),
    });
  const rowChoices = Math.min(HEADER_ROWS, preview.headerRow + preview.sample.length + 1);
  const missing = missingFields(preview, options);

  return (
    <Card>
      <h2 className="text-xl font-bold">{t("imports:steps.mapping")}</h2>
      <p className="text-ink-2">{t("imports:mapping.intro")}</p>
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {options.mapping.map((key, column) => {
          const title = preview.headers[column]?.trim() ?? "";
          const samples = preview.sample
            .map((row) => row[column]?.trim() ?? "")
            .filter((value) => value !== "")
            .slice(0, SAMPLES);
          return (
            <div key={column} className="flex flex-col gap-2 rounded-card border border-line p-3">
              <p className="truncate font-semibold" title={title}>
                {title === "" ? (
                  <span className="text-ink-2">{t("imports:mapping.untitled")}</span>
                ) : (
                  <bdi dir="auto">{title}</bdi>
                )}
              </p>
              {samples.length > 0 ? (
                <ul aria-label={t("imports:mapping.samples")} className="flex flex-col text-sm text-ink-2">
                  {samples.map((value, i) => (
                    <li key={i} className="truncate" title={value}>
                      <bdi dir="auto">{value}</bdi>
                    </li>
                  ))}
                </ul>
              ) : null}
              <Field id={`import-column-${column}`} label={t("imports:mapping.column", { n: column + 1 })}>
                <NativeSelect
                  id={`import-column-${column}`}
                  value={key ?? ""}
                  onChange={(event) =>
                    setField(column, event.target.value === "" ? null : event.target.value)
                  }
                >
                  <option value="">{t("imports:mapping.ignore")}</option>
                  {preview.fields.map((field) => (
                    <option key={field.key} value={field.key}>
                      {fieldLabel(field)}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
            </div>
          );
        })}
      </div>
      {missing.length > 0 ? (
        <p role="alert" className="rounded-control bg-warn-soft p-3 text-warn">
          {t("imports:mapping.missing", { fields: missing.map((f) => f.label).join(", ") })}
        </p>
      ) : null}
      <div className="grid gap-4 sm:grid-cols-2">
        <Field
          id="import-header-row"
          label={t("imports:mapping.headerRow")}
          hint={t("imports:mapping.headerRowHelp")}
        >
          <NativeSelect
            id="import-header-row"
            value={options.headerRow}
            onChange={(event) => onHeaderRow(Number(event.target.value))}
          >
            {Array.from({ length: rowChoices }, (_, row) => (
              <option key={row} value={row}>
                {formatNumber(row + 1, lang)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        {entity === "opening_stock" ? (
          <LocationSelect
            id="import-location"
            label={t("imports:mapping.location")}
            allLabel={t("imports:mapping.defaultLocation")}
            value={options.locationId ?? ""}
            onChange={(id) => onChange({ ...options, locationId: id === "" ? null : id })}
          />
        ) : null}
        {entity === "count_lines" ? (
          <CountSelect
            value={options.countId ?? ""}
            onChange={(id) => onChange({ ...options, countId: id === "" ? null : id })}
          />
        ) : null}
      </div>
      {POSTING_IMPORTS.has(entity) ? (
        <p className="text-ink-2">{t("imports:mapping.posting")}</p>
      ) : (
        <RadioCards
          legend={t("imports:mapping.policy")}
          name="import-policy"
          value={options.policy}
          onChange={(policy) => onChange({ ...options, policy })}
          options={policies.map((policy) => ({
            value: policy,
            label: t(`imports:mapping.policies.${policy}`),
            hint: t(`imports:mapping.policyHelp.${policy}`),
          }))}
        />
      )}
    </Card>
  );
}

/** The open stock count that receives the counted quantities. */
function CountSelect({ value, onChange }: { value: string; onChange: (countId: string) => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const { nameOf } = useLocations();
  const counts = useCounts("open").data ?? [];
  return (
    <Field
      id="import-count"
      label={t("imports:mapping.count")}
      hint={counts.length === 0 ? t("imports:mapping.countNone") : undefined}
    >
      <NativeSelect id="import-count" value={value} onChange={(event) => onChange(event.target.value)}>
        <option value="">{t("imports:mapping.countChoose")}</option>
        {counts.map((count) => (
          <option key={count.id} value={count.id}>
            {t("imports:mapping.countOption", {
              location: nameOf(count.locationId),
              date: formatDate(count.startedAt, lang),
            })}
          </option>
        ))}
      </NativeSelect>
    </Field>
  );
}
