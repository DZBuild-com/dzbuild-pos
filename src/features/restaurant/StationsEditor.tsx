import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useCategories } from "@/lib/ipc/catalog";
import { useArchiveStation, useSaveStation, useStations } from "@/lib/ipc/restaurant";
import type { Station, StationInput } from "@/lib/ipc/types";
import { type Trilingual, localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** The three names of a record (Arabic, French, English: printed in the language asked). */
export function NameFields({
  id,
  value,
  onChange,
}: {
  id: string;
  value: Trilingual;
  onChange: (value: Trilingual) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="grid gap-3 sm:grid-cols-3">
      <Field id={`${id}-fr`} label={t("restaurant:editor.nameFr")}>
        <Input
          id={`${id}-fr`}
          lang="fr"
          dir="ltr"
          value={value.nameFr}
          maxLength={60}
          onChange={(e) => onChange({ ...value, nameFr: e.target.value })}
        />
      </Field>
      <Field id={`${id}-ar`} label={t("restaurant:editor.nameAr")}>
        <Input
          id={`${id}-ar`}
          lang="ar"
          dir="rtl"
          value={value.nameAr}
          maxLength={60}
          onChange={(e) => onChange({ ...value, nameAr: e.target.value })}
        />
      </Field>
      <Field id={`${id}-en`} label={t("restaurant:editor.nameEn")}>
        <Input
          id={`${id}-en`}
          lang="en"
          dir="ltr"
          value={value.nameEn}
          maxLength={60}
          onChange={(e) => onChange({ ...value, nameEn: e.target.value })}
        />
      </Field>
    </div>
  );
}

const EMPTY: StationInput = { code: "", nameAr: "", nameFr: "", nameEn: "", sortOrder: 0, categoryIds: [] };

/** RST-006: the preparation stations (kitchen, grill, pizza, bar) and the categories each one
 * prepares: a line goes to the station of its product's category. */
export function StationsEditor() {
  const { t } = useTranslation();
  const lang = useLang();
  const stations = useStations();
  const [editing, setEditing] = useState<Station | "new" | null>(null);
  const list = (stations.data ?? []).filter((s) => !s.archived);
  return (
    <div className="flex flex-col gap-4 lg:flex-row">
      <div className="flex flex-col gap-2 lg:w-72">
        <Button onClick={() => setEditing("new")}>{t("restaurant:editor.addStation")}</Button>
        <ul className="flex flex-col gap-1">
          {list.map((station) => (
            <li key={station.id}>
              <button
                type="button"
                aria-pressed={editing !== "new" && editing?.id === station.id}
                onClick={() => setEditing(station)}
                className={cn(
                  "flex min-h-11 w-full items-center justify-between rounded-control border px-3 text-start font-semibold",
                  editing !== "new" && editing?.id === station.id
                    ? "border-brand bg-brand-soft"
                    : "border-line bg-surface",
                )}
              >
                <span>{localizedName(station, lang)}</span>
                <bdi dir="ltr" className="text-sm text-ink-2">
                  {station.code}
                </bdi>
              </button>
            </li>
          ))}
        </ul>
        {list.length === 0 && stations.isSuccess ? (
          <p className="text-ink-2">{t("restaurant:editor.noStations")}</p>
        ) : null}
      </div>
      {editing ? (
        <StationForm
          key={editing === "new" ? "new" : editing.id}
          station={editing === "new" ? null : editing}
          onDone={() => setEditing(null)}
        />
      ) : null}
    </div>
  );
}

function StationForm({ station, onDone }: { station: Station | null; onDone: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const categories = useCategories();
  const save = useSaveStation();
  const archive = useArchiveStation();
  const [input, setInput] = useState<StationInput>(
    station
      ? {
          code: station.code,
          nameAr: station.nameAr,
          nameFr: station.nameFr,
          nameEn: station.nameEn,
          sortOrder: station.sortOrder,
          categoryIds: station.categoryIds,
        }
      : EMPTY,
  );
  const valid = input.code.trim() !== "" && (input.nameFr.trim() !== "" || input.nameAr.trim() !== "");
  const toggle = (id: string, on: boolean) =>
    setInput((now) => ({
      ...now,
      categoryIds: on ? [...now.categoryIds, id] : now.categoryIds.filter((c) => c !== id),
    }));
  return (
    <form
      noValidate
      className="flex flex-1 flex-col gap-3 rounded-card border border-line bg-surface p-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (valid)
          save.mutate(
            { id: station?.id ?? null, input: { ...input, code: input.code.trim() } },
            { onSuccess: onDone },
          );
      }}
    >
      <Field id="station-code" label={t("restaurant:editor.code")}>
        <Input
          id="station-code"
          dir="ltr"
          value={input.code}
          maxLength={20}
          onChange={(e) => setInput({ ...input, code: e.target.value })}
        />
      </Field>
      <NameFields id="station-name" value={input} onChange={(names) => setInput({ ...input, ...names })} />
      <fieldset className="flex flex-col gap-2">
        <legend className="mb-1 font-semibold">{t("restaurant:editor.categories")}</legend>
        <div className="grid gap-2 sm:grid-cols-2">
          {(categories.data ?? [])
            .filter((c) => !c.archived)
            .map((category) => (
              <label key={category.id} className="flex min-h-11 items-center gap-3">
                <Checkbox
                  checked={input.categoryIds.includes(category.id)}
                  onCheckedChange={(v) => toggle(category.id, v === true)}
                />
                {localizedName(category, lang)}
              </label>
            ))}
        </div>
      </fieldset>
      <div className="flex flex-wrap gap-2">
        <Button type="submit" variant="primary" disabled={!valid || save.isPending}>
          {t("restaurant:editor.save")}
        </Button>
        {station ? (
          <Button
            variant="danger"
            disabled={archive.isPending}
            onClick={() => archive.mutate(station.id, { onSuccess: onDone })}
          >
            {t("restaurant:editor.archive")}
          </Button>
        ) : null}
        <Button variant="ghost" onClick={onDone}>
          {t("common:actions.cancel")}
        </Button>
      </div>
      {save.error || archive.error ? (
        <p role="alert" className="text-danger">
          {errorText(save.error ?? archive.error)}
        </p>
      ) : null}
    </form>
  );
}
