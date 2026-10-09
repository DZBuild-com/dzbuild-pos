import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useDuplicateRole } from "@/lib/ipc/completion";
import type { Permission, Role } from "@/lib/ipc/types";
import { useCreateRole, useRoleCatalog, useUpdateRole } from "@/lib/ipc/users";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** USR-015: a custom role with the permissions of `role`, under three new names (the owner's,
 * like every custom role). */
export function DuplicateRoleDialog({ role, onClose }: { role: Role; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const duplicate = useDuplicateRole();
  const [names, setNames] = useState({ nameAr: "", nameFr: "", nameEn: "" });
  const [nameError, setNameError] = useState(false);
  const name = localizedName(role, lang);
  const save = () => {
    if (![names.nameAr, names.nameFr, names.nameEn].some((n) => n.trim() !== "")) {
      setNameError(true);
      return;
    }
    setNameError(false);
    duplicate.mutate(
      { roleId: role.id, names },
      {
        onSuccess: () => {
          toast.success(t("config:roles.duplicated"));
          onClose();
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t("config:roles.duplicateTitle", { name })}
      description={t("config:roles.duplicateHint")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" onClick={save} disabled={duplicate.isPending}>
            {t("config:roles.duplicateSave")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 sm:grid-cols-3">
        <Field id="role-copy-ar" label={t("settings:roles.nameAr")}>
          <Input
            id="role-copy-ar"
            dir="rtl"
            lang="ar"
            value={names.nameAr}
            onChange={(e) => setNames({ ...names, nameAr: e.target.value })}
          />
        </Field>
        <Field
          id="role-copy-fr"
          label={t("settings:roles.nameFr")}
          error={nameError ? t("validation:nameRequired") : undefined}
        >
          <Input
            id="role-copy-fr"
            dir="ltr"
            value={names.nameFr}
            onChange={(e) => setNames({ ...names, nameFr: e.target.value })}
          />
        </Field>
        <Field id="role-copy-en" label={t("settings:roles.nameEn")}>
          <Input
            id="role-copy-en"
            dir="ltr"
            value={names.nameEn}
            onChange={(e) => setNames({ ...names, nameEn: e.target.value })}
          />
        </Field>
      </div>
      {duplicate.error ? (
        <p role="alert" className="text-danger">
          {errorText(duplicate.error)}
        </p>
      ) : null}
    </Dialog>
  );
}

/** Checkboxes grouped as in spec §4.5, in plain words; the owner role is read-only. */
export function RoleDialog({ role, onClose }: { role: Role | null; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const catalog = useRoleCatalog();
  const create = useCreateRole();
  const update = useUpdateRole();
  const [names, setNames] = useState({
    nameAr: role?.nameAr ?? "",
    nameFr: role?.nameFr ?? "",
    nameEn: role?.nameEn ?? "",
  });
  const [granted, setGranted] = useState<Set<Permission>>(() => new Set(role?.permissions ?? []));
  const [nameError, setNameError] = useState(false);
  const locked = role?.isOwner ?? false;
  const mutation = role ? update : create;

  const toggle = (permission: Permission, on: boolean) =>
    setGranted((previous) => {
      const next = new Set(previous);
      if (on) next.add(permission);
      else next.delete(permission);
      return next;
    });
  const save = () => {
    if (![names.nameAr, names.nameFr, names.nameEn].some((n) => n.trim() !== "")) {
      setNameError(true);
      return;
    }
    setNameError(false);
    const input = { ...names, permissions: [...granted] };
    const done = {
      onSuccess: () => {
        toast.success(t("settings:roles.saved"));
        onClose();
      },
    };
    if (role) update.mutate({ id: role.id, input }, done);
    else create.mutate(input, done);
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t(role ? "settings:roles.edit" : "settings:roles.add")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t(locked ? "common:actions.close" : "common:actions.cancel")}
          </Button>
          {locked ? null : (
            <Button variant="primary" onClick={save} disabled={mutation.isPending}>
              {t("settings:roles.save")}
            </Button>
          )}
        </>
      }
    >
      {locked ? <p className="rounded-control bg-brand-soft p-3">{t("settings:roles.ownerAll")}</p> : null}
      <div className="grid gap-4 sm:grid-cols-3">
        <Field id="role-name-ar" label={t("settings:roles.nameAr")}>
          <Input
            id="role-name-ar"
            dir="rtl"
            lang="ar"
            disabled={locked}
            value={names.nameAr}
            onChange={(e) => setNames({ ...names, nameAr: e.target.value })}
          />
        </Field>
        <Field
          id="role-name-fr"
          label={t("settings:roles.nameFr")}
          error={nameError ? t("validation:nameRequired") : undefined}
        >
          <Input
            id="role-name-fr"
            dir="ltr"
            disabled={locked}
            value={names.nameFr}
            onChange={(e) => setNames({ ...names, nameFr: e.target.value })}
          />
        </Field>
        <Field id="role-name-en" label={t("settings:roles.nameEn")}>
          <Input
            id="role-name-en"
            dir="ltr"
            disabled={locked}
            value={names.nameEn}
            onChange={(e) => setNames({ ...names, nameEn: e.target.value })}
          />
        </Field>
      </div>
      <p className="font-semibold">{t("settings:roles.permissions")}</p>
      <div className="grid gap-4 md:grid-cols-2">
        {catalog.data?.map((group) => (
          <fieldset key={group.group} className="flex flex-col gap-1 rounded-card border border-line p-3">
            <legend className="px-1 font-semibold">{t(`auth:permissions.groups.${group.group}`)}</legend>
            {group.permissions.map((permission) => (
              <label key={permission} className="flex min-h-11 items-center gap-3">
                <Checkbox
                  checked={locked || granted.has(permission)}
                  disabled={locked}
                  onCheckedChange={(value) => toggle(permission, value === true)}
                />
                <span>{t(`auth:permissions.${permission}`)}</span>
              </label>
            ))}
          </fieldset>
        ))}
      </div>
      {mutation.error ? (
        <p role="alert" className="text-danger">
          {errorText(mutation.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
