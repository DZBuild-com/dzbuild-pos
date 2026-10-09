import { Copy, Plus, Trash2, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Splash } from "@/components/splash";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { ScanInput } from "@/components/scan-input";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { useCategories } from "@/lib/ipc/catalog";
import { toAppError } from "@/lib/ipc/client";
import { useLabelPreview, useLabelTemplates, useSaveLabelTemplates } from "@/lib/ipc/labels";
import { useProductSearch } from "@/lib/ipc/products";
import type { LabelFields, LabelTemplate, LabelTemplatesView, SymbologyChoice } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty, parseQtyOrZero } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { useTillLink } from "@/lib/ipc/lan";
import { useModule } from "@/lib/ipc/packs";
import { cn } from "@/lib/utils";
import { LABEL_LANGS } from "./PrintLabelsDialog";
import { useTemplateName } from "./template-name";

const FIELDS = [
  "name",
  "price",
  "unit",
  "barcode",
  "reference",
  "lot",
  "expiry",
  "promo",
  "shop",
  "weight",
  "packedOn",
] as const satisfies readonly (keyof LabelFields)[];
/** LBL-004: Code 39 prints the product's reference, ITF its barcode digits (cartons). */
const SYMBOLOGIES: readonly SymbologyChoice[] = ["auto", "code128", "code39", "itf", "qr", "none"];
/** VRT-143: the piece's metal, fineness, weight and hallmark, offered while the jewellery module is
 * on. */
const JEWEL_FIELDS = [...FIELDS, "jewel"] as const satisfies readonly (keyof LabelFields)[];
const COLUMNS = [1, 2, 3];
const NAME_LINES = [1, 2, 3];
/** pos-core `labels::MAX_TEMPLATES` and the 40 characters of a template's name. */
const MAX_TEMPLATES = 50;
const MAX_NAME = 40;
/** Sizes travel as `u16` tenths of a millimetre. */
const MAX_DMM = 65_535;
const CUSTOM = "custom";

/** A new shop template: 40 × 25 mm, Arabic + French (DOC-034 safe), name, price and barcode. */
function blankTemplate(id: string, name: string): LabelTemplate {
  const fields = Object.fromEntries(FIELDS.map((f) => [f, false])) as Required<LabelFields>;
  return {
    id,
    name,
    widthDmm: 400,
    heightDmm: 250,
    gapDmm: 20,
    columns: 1,
    columnGapDmm: 0,
    lang: "ar_fr",
    fields: { ...fields, name: true, price: true, unit: true, barcode: true },
    symbology: "auto",
    nameLines: 1,
  };
}

/** `custom-1`, `custom-2`…: the first id no template has (pos-core accepts `[a-z0-9._-]`). */
function nextId(list: readonly LabelTemplate[]): string {
  const taken = new Set(list.map((tpl) => tpl.id));
  let n = 1;
  while (taken.has(`custom-${n}`)) n += 1;
  return `custom-${n}`;
}

/** A length in millimetres with one decimal, kept as tenths (`dmm`). What is typed stays in the
 * field while it is being written ("50," on the way to "50,5"); each valid value updates the draft. */
function MillimetreField({
  id,
  label,
  dmm,
  error,
  onChange,
}: {
  id: string;
  label: string;
  dmm: number;
  error?: string;
  onChange: (dmm: number) => void;
}) {
  const lang = useLang();
  // 1 mm = 10 dmm = 1 000 milli-units of a one-decimal quantity
  const [text, setText] = useState(() => formatQty(dmm * 100, lang));
  const milli = parseQtyOrZero(text, 1);
  return (
    <Field id={id} label={label} error={error}>
      <NumericInput
        id={id}
        kind="qty"
        aria-invalid={milli === null || milli / 100 > MAX_DMM}
        value={text}
        onValueChange={(value) => {
          setText(value);
          const typed = parseQtyOrZero(value, 1);
          if (typed !== null && typed / 100 <= MAX_DMM) onChange(typed / 100);
        }}
      />
    </Field>
  );
}

/** LBL-001/LBL-005 designer-lite: the shop's label templates (size, 1- to 3-up, language, printed
 * fields, barcode) with a live preview drawn by Rust on a real product, and the template of each
 * category. Nothing is stored before "Enregistrer les modèles". */
export function LabelDesigner() {
  const errorText = useErrorText();
  const loaded = useLabelTemplates();
  if (loaded.isPending) return <Splash inline />;
  if (loaded.isError) {
    return (
      <p role="alert" className="text-danger">
        {errorText(loaded.error)}
      </p>
    );
  }
  return <Designer view={loaded.data} />;
}

function Designer({ view }: { view: LabelTemplatesView }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const templateName = useTemplateName();
  const categories = (useCategories().data ?? []).filter((c) => !c.archived);
  const save = useSaveLabelTemplates();
  // The templates are the main PC's; on a till they are shown, previewed, not edited.
  const readOnly = useTillLink().isTill;
  const fields = useModule("jewellery") ? JEWEL_FIELDS : FIELDS;
  // Loaded once: the designer edits this draft, which only `labels_save_templates` stores.
  const [list, setList] = useState<LabelTemplate[]>(view.templates);
  const [byCategory, setByCategory] = useState<Record<string, string>>(view.categoryTemplates);
  const [selectedId, setSelectedId] = useState(view.templates[0]?.id ?? "");
  const [custom, setCustom] = useState(false);
  // bumped when a preset or another template replaces the sizes: the millimetre fields start again
  const [resets, setResets] = useState(0);
  const [picked, setPicked] = useState<{ id: string; name: string } | null>(null);
  const [search, setSearch] = useState("");
  const query = useDebounced(search.trim(), 200);
  const hits = useProductSearch(
    { text: query, limit: 6, categoryId: null, includeArchived: false },
    query.length >= 2,
  );
  const draft = list.find((tpl) => tpl.id === selectedId) ?? list[0] ?? null;
  const preview = useLabelPreview(
    draft ? { template: draft, productId: picked?.id ?? null, lang: null } : null,
  );
  const failed = preview.error ? toAppError(preview.error) : null;
  const fieldError = (field: keyof LabelTemplate) =>
    failed?.field === `template.${field}` ? errorText(failed) : undefined;
  const shownFields: readonly string[] = ["name", "widthDmm", "heightDmm", "gapDmm", "columnGapDmm"];
  const otherError =
    failed && !shownFields.some((f) => failed.field === `template.${f}`) ? errorText(failed) : null;
  const dropped = (preview.data?.warnings ?? []).some((w) => w.code === "lot_or_expiry_dropped");

  const update = (change: Partial<LabelTemplate>) => {
    if (!draft) return;
    setList((all) => all.map((tpl) => (tpl.id === draft.id ? { ...tpl, ...change } : tpl)));
    save.reset();
  };
  const select = (id: string) => {
    setSelectedId(id);
    setCustom(false);
    setResets((n) => n + 1);
  };
  const add = (template: LabelTemplate) => {
    setList((all) => [...all, template]);
    select(template.id);
    save.reset();
  };
  const remove = () => {
    if (!draft || list.length <= 1) return;
    const rest = list.filter((tpl) => tpl.id !== draft.id);
    setList(rest);
    setByCategory((all) => Object.fromEntries(Object.entries(all).filter(([, id]) => id !== draft.id)));
    select(rest[0]?.id ?? "");
    save.reset();
  };

  const preset = draft
    ? view.sizePresets.find((p) => p.widthMm * 10 === draft.widthDmm && p.heightMm * 10 === draft.heightDmm)
    : undefined;
  const sizeValue = custom || !preset ? CUSTOM : `${preset.widthMm}x${preset.heightMm}`;
  const full = list.length >= MAX_TEMPLATES;

  return (
    <div className="flex flex-col gap-6">
      <HostOnlyNote />
      <div className="grid gap-6 lg:grid-cols-[16rem_minmax(0,1fr)]">
        <Card aria-labelledby="label-templates-title" className="gap-3 p-4">
          <h2 id="label-templates-title" className="text-lg font-bold">
            {t("labels:designer.list")}
          </h2>
          <div className="flex flex-col gap-1">
            {list.map((tpl) => (
              <button
                key={tpl.id}
                type="button"
                aria-pressed={draft?.id === tpl.id}
                onClick={() => select(tpl.id)}
                className={cn(
                  "min-h-11 rounded-control px-3 text-start font-semibold",
                  draft?.id === tpl.id ? "bg-brand-soft" : "hover:bg-surface-2",
                )}
              >
                {templateName(tpl)}
              </button>
            ))}
          </div>
          {readOnly ? null : (
            <div className="flex flex-wrap gap-2">
              <Button
                variant="outline"
                disabled={full}
                onClick={() => add(blankTemplate(nextId(list), t("labels:designer.new")))}
              >
                <Plus aria-hidden />
                {t("labels:designer.new")}
              </Button>
              <Button
                variant="outline"
                disabled={full || !draft}
                onClick={() => {
                  if (!draft) return;
                  const name = t("labels:designer.copyOf", { name: templateName(draft) }).slice(0, MAX_NAME);
                  add({ ...draft, id: nextId(list), name });
                }}
              >
                <Copy aria-hidden />
                {t("labels:designer.duplicate")}
              </Button>
              <Button variant="danger" disabled={list.length <= 1} onClick={remove}>
                <Trash2 aria-hidden />
                {t("labels:designer.remove")}
              </Button>
            </div>
          )}
        </Card>
        {draft ? (
          <fieldset disabled={readOnly} className="contents">
            <Card className="gap-6">
              <div className="grid gap-4 sm:grid-cols-2">
                <Field id="label-name" label={t("labels:designer.templateName")} error={fieldError("name")}>
                  <Input
                    id="label-name"
                    dir="auto"
                    maxLength={MAX_NAME}
                    value={draft.name}
                    placeholder={templateName({ ...draft, name: "" })}
                    onChange={(e) => update({ name: e.target.value })}
                  />
                </Field>
                <Field
                  id="label-size"
                  label={t("labels:designer.size")}
                  // a preset too small for what the template prints: said here, where it is chosen
                  error={
                    sizeValue === CUSTOM ? undefined : (fieldError("widthDmm") ?? fieldError("heightDmm"))
                  }
                >
                  <NativeSelect
                    id="label-size"
                    value={sizeValue}
                    onChange={(e) => {
                      const chosen = view.sizePresets.find(
                        (p) => `${p.widthMm}x${p.heightMm}` === e.target.value,
                      );
                      if (!chosen) {
                        setCustom(true);
                        return;
                      }
                      update({ widthDmm: chosen.widthMm * 10, heightDmm: chosen.heightMm * 10 });
                      setCustom(false);
                      setResets((n) => n + 1);
                    }}
                  >
                    {view.sizePresets.map((p) => (
                      <option key={`${p.widthMm}x${p.heightMm}`} value={`${p.widthMm}x${p.heightMm}`}>
                        {t("labels:designer.sizeOption", { width: p.widthMm, height: p.heightMm })}
                      </option>
                    ))}
                    <option value={CUSTOM}>{t("labels:designer.custom")}</option>
                  </NativeSelect>
                </Field>
                {sizeValue === CUSTOM ? (
                  <>
                    <MillimetreField
                      key={`${draft.id}-${resets}-width`}
                      id="label-width"
                      label={t("labels:designer.width")}
                      dmm={draft.widthDmm}
                      error={fieldError("widthDmm")}
                      onChange={(widthDmm) => {
                        // a typed size that happens to be a preset keeps its fields in view
                        setCustom(true);
                        update({ widthDmm });
                      }}
                    />
                    <MillimetreField
                      key={`${draft.id}-${resets}-height`}
                      id="label-height"
                      label={t("labels:designer.height")}
                      dmm={draft.heightDmm}
                      error={fieldError("heightDmm")}
                      onChange={(heightDmm) => {
                        setCustom(true);
                        update({ heightDmm });
                      }}
                    />
                  </>
                ) : null}
                <MillimetreField
                  key={`${draft.id}-${resets}-gap`}
                  id="label-gap"
                  label={t("labels:designer.gap")}
                  dmm={draft.gapDmm}
                  error={fieldError("gapDmm")}
                  onChange={(gapDmm) => update({ gapDmm })}
                />
                <Field id="label-columns" label={t("labels:designer.columns")}>
                  <NativeSelect
                    id="label-columns"
                    value={draft.columns}
                    onChange={(e) => update({ columns: Number(e.target.value) })}
                  >
                    {COLUMNS.map((n) => (
                      <option key={n} value={n}>
                        {n}
                      </option>
                    ))}
                  </NativeSelect>
                </Field>
                {draft.columns > 1 ? (
                  <MillimetreField
                    key={`${draft.id}-${resets}-columnGap`}
                    id="label-column-gap"
                    label={t("labels:designer.columnGap")}
                    dmm={draft.columnGapDmm}
                    error={fieldError("columnGapDmm")}
                    onChange={(columnGapDmm) => update({ columnGapDmm })}
                  />
                ) : null}
                <Field id="label-template-lang" label={t("labels:print.lang")}>
                  <NativeSelect
                    id="label-template-lang"
                    value={draft.lang}
                    onChange={(e) => {
                      const chosen = LABEL_LANGS.find((l) => l === e.target.value);
                      if (chosen) update({ lang: chosen });
                    }}
                  >
                    {LABEL_LANGS.map((l) => (
                      <option key={l} value={l}>
                        {t(`labels:langs.${l}`)}
                      </option>
                    ))}
                  </NativeSelect>
                </Field>
                <Field id="label-symbology" label={t("labels:designer.symbology")}>
                  <NativeSelect
                    id="label-symbology"
                    value={draft.symbology}
                    onChange={(e) => {
                      const chosen = SYMBOLOGIES.find((s) => s === e.target.value);
                      if (chosen) update({ symbology: chosen });
                    }}
                  >
                    {SYMBOLOGIES.map((s) => (
                      <option key={s} value={s}>
                        {t(`labels:designer.symbologies.${s}`)}
                      </option>
                    ))}
                  </NativeSelect>
                </Field>
                <Field id="label-name-lines" label={t("labels:designer.nameLines")}>
                  <NativeSelect
                    id="label-name-lines"
                    value={draft.nameLines}
                    onChange={(e) => update({ nameLines: Number(e.target.value) })}
                  >
                    {NAME_LINES.map((n) => (
                      <option key={n} value={n}>
                        {n}
                      </option>
                    ))}
                  </NativeSelect>
                </Field>
              </div>
              <fieldset className="flex flex-col gap-3">
                <legend className="mb-3 font-semibold">{t("labels:designer.printed")}</legend>
                <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                  {fields.map((field) => (
                    <label key={field} className="flex min-h-11 items-center gap-3">
                      <Checkbox
                        checked={draft.fields[field] ?? false}
                        onCheckedChange={(on) =>
                          update({ fields: { ...draft.fields, [field]: on === true } })
                        }
                      />
                      {field === "jewel"
                        ? t("jewellery:labels.jewelLine")
                        : t(`labels:designer.fields.${field}`)}
                    </label>
                  ))}
                </div>
              </fieldset>
              <div className="grid gap-4 sm:grid-cols-2">
                <div className="flex flex-col gap-2">
                  <Field
                    id="label-preview-on"
                    label={t("labels:designer.previewOn")}
                    hint={picked ? picked.name : t("labels:designer.previewFirst")}
                  >
                    <ScanInput
                      id="label-preview-on"
                      type="search"
                      dir="auto"
                      value={search}
                      onChange={(e) => setSearch(e.target.value)}
                      onScan={setSearch}
                    />
                  </Field>
                  {picked ? (
                    <Button variant="ghost" className="self-start" onClick={() => setPicked(null)}>
                      <X aria-hidden />
                      {t("labels:designer.previewReset")}
                    </Button>
                  ) : null}
                  {search.trim().length >= 2 ? (
                    <div className="flex flex-col gap-1">
                      {(hits.data ?? []).map((hit) => (
                        <button
                          key={hit.id}
                          type="button"
                          onClick={() => {
                            setPicked({ id: hit.id, name: localizedName(hit, lang) });
                            setSearch("");
                          }}
                          className="min-h-11 rounded-control px-3 text-start hover:bg-surface-2"
                        >
                          {localizedName(hit, lang)}
                        </button>
                      ))}
                    </div>
                  ) : null}
                </div>
                <div className="flex flex-col items-center gap-2">
                  {preview.data ? (
                    <img
                      src={preview.data.png}
                      alt={t("labels:designer.previewAlt")}
                      className="max-h-64 max-w-full border border-line bg-surface"
                    />
                  ) : null}
                  {dropped ? <p className="text-warn">{t("labels:print.dropped")}</p> : null}
                  {otherError ? <p className="text-danger">{otherError}</p> : null}
                </div>
              </div>
            </Card>
          </fieldset>
        ) : null}
      </div>
      {categories.length > 0 ? (
        <fieldset disabled={readOnly} className="contents">
          <Card aria-labelledby="label-categories-title">
            <h2 id="label-categories-title" className="text-lg font-bold">
              {t("labels:designer.categories")}
            </h2>
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
              {categories.map((category) => (
                <Field
                  key={category.id}
                  id={`label-category-${category.id}`}
                  label={localizedName(category, lang)}
                >
                  <NativeSelect
                    id={`label-category-${category.id}`}
                    value={byCategory[category.id] ?? ""}
                    onChange={(e) => {
                      const id = e.target.value;
                      setByCategory((all) => {
                        const next = { ...all };
                        if (id === "") delete next[category.id];
                        else next[category.id] = id;
                        return next;
                      });
                      save.reset();
                    }}
                  >
                    <option value="">{t("labels:designer.default")}</option>
                    {list.map((tpl) => (
                      <option key={tpl.id} value={tpl.id}>
                        {templateName(tpl)}
                      </option>
                    ))}
                  </NativeSelect>
                </Field>
              ))}
            </div>
          </Card>
        </fieldset>
      ) : null}
      {readOnly ? null : (
        <div className="flex flex-col gap-2">
          <div>
            <Button
              variant="primary"
              disabled={save.isPending}
              onClick={() => save.mutate({ templates: list, categoryTemplates: byCategory })}
            >
              {t("labels:designer.save")}
            </Button>
          </div>
          {save.isSuccess ? (
            <p role="status" className="text-ok">
              {t("labels:designer.saved")}
            </p>
          ) : null}
          {save.error ? (
            <p role="alert" className="text-danger">
              {errorText(save.error)}
            </p>
          ) : null}
        </div>
      )}
    </div>
  );
}
