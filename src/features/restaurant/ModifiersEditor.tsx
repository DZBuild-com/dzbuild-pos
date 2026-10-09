import { X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { useProduct } from "@/lib/ipc/products";
import {
  useArchiveModifierGroup,
  useModifierGroups,
  useModifiersFor,
  useSaveModifierGroup,
  useSetProductModifiers,
} from "@/lib/ipc/restaurant";
import type { ModifierGroup, ModifierOptionInput, ProductHit } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { ProductPicker } from "./ProductPicker";
import { NameFields } from "./StationsEditor";

/** SAL-024, PRD-021: the option groups (sauces, cooking, supplements) with how many the waiter
 * must and may choose, their options (a priced option is a product of its own), and which
 * groups each product asks at the till. */
export function ModifiersEditor() {
  const { t } = useTranslation();
  const lang = useLang();
  const groups = useModifierGroups();
  const [editing, setEditing] = useState<ModifierGroup | "new" | null>(null);
  const list = (groups.data ?? []).filter((g) => !g.archived);
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-4 lg:flex-row">
        <div className="flex flex-col gap-2 lg:w-72">
          <Button onClick={() => setEditing("new")}>{t("restaurant:editor.addGroup")}</Button>
          <ul className="flex flex-col gap-1">
            {list.map((group) => (
              <li key={group.id}>
                <button
                  type="button"
                  aria-pressed={editing !== "new" && editing?.id === group.id}
                  onClick={() => setEditing(group)}
                  className={cn(
                    "flex min-h-11 w-full items-center rounded-control border px-3 text-start font-semibold",
                    editing !== "new" && editing?.id === group.id
                      ? "border-brand bg-brand-soft"
                      : "border-line bg-surface",
                  )}
                >
                  {localizedName(group, lang)}
                </button>
              </li>
            ))}
          </ul>
          {list.length === 0 && groups.isSuccess ? (
            <p className="text-ink-2">{t("restaurant:editor.noGroups")}</p>
          ) : null}
        </div>
        {editing ? (
          <GroupForm
            key={editing === "new" ? "new" : editing.id}
            group={editing === "new" ? null : editing}
            onDone={() => setEditing(null)}
          />
        ) : null}
      </div>
      <ProductGroups groups={list} />
    </div>
  );
}

interface OptionDraft extends ModifierOptionInput {
  key: string;
}

let drafts = 0;
const draftKey = () => `option-${(drafts += 1)}`;

function GroupForm({ group, onDone }: { group: ModifierGroup | null; onDone: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const save = useSaveModifierGroup();
  const archive = useArchiveModifierGroup();
  const [names, setNames] = useState({
    nameAr: group?.nameAr ?? "",
    nameFr: group?.nameFr ?? "",
    nameEn: group?.nameEn ?? "",
  });
  const [min, setMin] = useState(String(group?.minSelect ?? 0));
  const [max, setMax] = useState(String(group?.maxSelect ?? 1));
  const [options, setOptions] = useState<OptionDraft[]>(
    (group?.options ?? [])
      .filter((o) => !o.archived)
      .map((o) => ({
        key: o.id,
        id: o.id,
        nameAr: o.nameAr,
        nameFr: o.nameFr,
        nameEn: o.nameEn,
        productId: o.productId,
        sortOrder: o.sortOrder,
      })),
  );
  const minSelect = Number(min);
  const maxSelect = Number(max);
  const named = (o: { nameFr: string; nameAr: string }) => o.nameFr.trim() !== "" || o.nameAr.trim() !== "";
  const valid =
    named(names) &&
    options.length > 0 &&
    options.every(named) &&
    Number.isInteger(minSelect) &&
    Number.isInteger(maxSelect) &&
    maxSelect >= 1 &&
    minSelect <= maxSelect &&
    minSelect <= options.length;
  const update = (key: string, change: Partial<OptionDraft>) =>
    setOptions((all) => all.map((o) => (o.key === key ? { ...o, ...change } : o)));
  const submit = () =>
    save.mutate(
      {
        id: group?.id ?? null,
        input: {
          ...names,
          minSelect,
          maxSelect,
          sortOrder: group?.sortOrder ?? 0,
          options: options.map((o, index) => ({
            id: o.id,
            nameAr: o.nameAr,
            nameFr: o.nameFr,
            nameEn: o.nameEn,
            productId: o.productId,
            sortOrder: index + 1,
          })),
        },
      },
      { onSuccess: onDone },
    );
  return (
    <form
      noValidate
      className="flex flex-1 flex-col gap-3 rounded-card border border-line bg-surface p-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (valid) submit();
      }}
    >
      <NameFields id="group-name" value={names} onChange={setNames} />
      <div className="grid grid-cols-2 gap-3 sm:w-80">
        <Field id="group-min" label={t("restaurant:editor.min")}>
          <NumericInput id="group-min" kind="integer" value={min} onValueChange={setMin} />
        </Field>
        <Field id="group-max" label={t("restaurant:editor.max")}>
          <NumericInput id="group-max" kind="integer" value={max} onValueChange={setMax} />
        </Field>
      </div>
      <fieldset className="flex flex-col gap-3">
        <legend className="mb-1 font-semibold">{t("restaurant:editor.options")}</legend>
        {options.map((option, index) => (
          <div key={option.key} className="flex flex-col gap-2 rounded-control border border-line p-3">
            <div className="flex items-start gap-2">
              <div className="flex-1">
                <NameFields
                  id={`option-${index}`}
                  value={option}
                  onChange={(value) => update(option.key, value)}
                />
              </div>
              <Button
                variant="ghost"
                size="icon"
                aria-label={t("restaurant:editor.removeOption")}
                onClick={() => setOptions((all) => all.filter((o) => o.key !== option.key))}
              >
                <X aria-hidden />
              </Button>
            </div>
            {option.productId ? (
              <OptionProduct
                productId={option.productId}
                onClear={() => update(option.key, { productId: null })}
              />
            ) : (
              <ProductPicker
                id={`option-product-${index}`}
                label={t("restaurant:editor.optionProduct")}
                onPick={(hit) => update(option.key, { productId: hit.id })}
              />
            )}
          </div>
        ))}
        <Button
          variant="outline"
          className="self-start"
          onClick={() =>
            setOptions((all) => [
              ...all,
              {
                key: draftKey(),
                id: null,
                nameAr: "",
                nameFr: "",
                nameEn: "",
                productId: null,
                sortOrder: 0,
              },
            ])
          }
        >
          {t("restaurant:editor.addOption")}
        </Button>
      </fieldset>
      <div className="flex flex-wrap gap-2">
        <Button type="submit" variant="primary" disabled={!valid || save.isPending}>
          {t("restaurant:editor.save")}
        </Button>
        {group ? (
          <Button
            variant="danger"
            disabled={archive.isPending}
            onClick={() => archive.mutate(group.id, { onSuccess: onDone })}
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

/** The product a priced option sells. */
function OptionProduct({ productId, onClear }: { productId: string; onClear: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const product = useProduct(productId);
  return (
    <p className="flex items-center gap-2 text-sm">
      <span>
        {t("restaurant:editor.optionSells", { name: product.data ? localizedName(product.data, lang) : "" })}
      </span>
      <Button variant="ghost" size="icon" aria-label={t("restaurant:editor.clearProduct")} onClick={onClear}>
        <X aria-hidden />
      </Button>
    </p>
  );
}

/** Which groups a product asks at the till, in this order. */
function ProductGroups({ groups }: { groups: ModifierGroup[] }) {
  const { t } = useTranslation();
  const lang = useLang();
  const [product, setProduct] = useState<ProductHit | null>(null);
  return (
    <section
      aria-labelledby="product-groups"
      className="flex flex-col gap-3 rounded-card border border-line bg-surface p-4"
    >
      <h3 id="product-groups" className="text-lg font-bold">
        {t("restaurant:editor.productGroups")}
      </h3>
      <ProductPicker id="product-groups-search" label={t("restaurant:editor.product")} onPick={setProduct} />
      {product ? (
        <ProductGroupChoice
          key={product.id}
          productId={product.id}
          name={localizedName(product, lang)}
          groups={groups}
        />
      ) : null}
    </section>
  );
}

function ProductGroupChoice({
  productId,
  name,
  groups,
}: {
  productId: string;
  name: string;
  groups: ModifierGroup[];
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const current = useModifiersFor(productId);
  const set = useSetProductModifiers();
  const [chosen, setChosen] = useState<string[] | null>(null);
  const ids = chosen ?? (current.data ?? []).map((g) => g.id);
  const toggle = (id: string, on: boolean) => setChosen(on ? [...ids, id] : ids.filter((x) => x !== id));
  return (
    <div className="flex flex-col gap-2">
      <p className="font-semibold">{name}</p>
      {groups.map((group) => (
        <label key={group.id} className="flex min-h-11 items-center gap-3">
          <Checkbox checked={ids.includes(group.id)} onCheckedChange={(v) => toggle(group.id, v === true)} />
          {localizedName(group, lang)}
        </label>
      ))}
      <Button
        variant="primary"
        className="self-start"
        disabled={chosen === null || set.isPending}
        onClick={() => set.mutate({ productId, groupIds: ids }, { onSuccess: () => setChosen(null) })}
      >
        {t("restaurant:editor.save")}
      </Button>
      {set.error ? (
        <p role="alert" className="text-danger">
          {errorText(set.error)}
        </p>
      ) : null}
    </div>
  );
}
