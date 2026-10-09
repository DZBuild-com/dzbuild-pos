import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useArchiveCategory, useCategories, useCreateCategory, useUpdateCategory } from "@/lib/ipc/catalog";
import type { Category } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** Categories for the sale screen chips (SAL-010): trilingual names, archive. */
export function CategoriesDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const categories = (useCategories().data ?? []).filter((c) => !c.archived);
  const create = useCreateCategory();
  const update = useUpdateCategory();
  const archive = useArchiveCategory();
  const [editing, setEditing] = useState<Category | null>(null);
  const [names, setNames] = useState({ nameAr: "", nameFr: "", nameEn: "" });
  const start = (c: Category | null) => {
    setEditing(c);
    setNames(
      c ? { nameAr: c.nameAr, nameFr: c.nameFr, nameEn: c.nameEn } : { nameAr: "", nameFr: "", nameEn: "" },
    );
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const input = {
      parentId: null,
      ...names,
      color: editing?.color ?? null,
      sortOrder: editing?.sortOrder ?? categories.length + 1,
      image: null,
    };
    if (editing) update.mutate({ id: editing.id, input }, { onSuccess: () => start(null) });
    else create.mutate(input, { onSuccess: () => start(null) });
  };
  const error = create.error ?? update.error ?? archive.error;
  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("products:categories.title")}
    >
      <ul className="flex flex-col gap-2">
        {categories.map((c) => (
          <li key={c.id} className="flex items-center justify-between rounded-card border border-line p-2">
            <span className="font-semibold">{localizedName(c, lang)}</span>
            <span className="flex gap-2">
              <Button variant="outline" onClick={() => start(c)}>
                {t("common:actions.edit")}
              </Button>
              <Button variant="ghost" onClick={() => archive.mutate(c.id)}>
                {t("products:categories.archive")}
              </Button>
            </span>
          </li>
        ))}
      </ul>
      <form noValidate onSubmit={submit} className="grid gap-3 sm:grid-cols-3">
        {(["nameFr", "nameAr", "nameEn"] as const).map((k) => (
          <Field key={k} id={`cat-${k}`} label={t(`products:fields.${k}`)}>
            <Input
              id={`cat-${k}`}
              dir={k === "nameAr" ? "rtl" : "auto"}
              value={names[k]}
              onChange={(e) => setNames({ ...names, [k]: e.target.value })}
            />
          </Field>
        ))}
        <div className="sm:col-span-3">
          <Button type="submit" variant="primary">
            {editing ? t("products:categories.save") : t("products:categories.add")}
          </Button>
        </div>
      </form>
      {error ? (
        <p role="alert" className="text-danger">
          {errorText(error)}
        </p>
      ) : null}
    </Dialog>
  );
}
