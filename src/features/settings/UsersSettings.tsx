import { Copy, KeyRound, Pencil, Plus, UserCheck, UserX } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Avatar } from "@/components/avatar";
import { PinInput } from "@/components/pin-pad";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { formatDate, formatPercentInput, parsePercentBp } from "@/lib/format";
import { isLang, type Lang, LANGS } from "@/lib/i18n";
import { useDuplicateUser } from "@/lib/ipc/completion";
import { useAutoLockMinutes, useSetSetting } from "@/lib/ipc/settings";
import type { Role, Session, User } from "@/lib/ipc/types";
import {
  useCreateUser,
  useDeactivateUser,
  useDeleteRole,
  useRoles,
  useSetUserPin,
  useUpdateUser,
  useUsers,
} from "@/lib/ipc/users";
import { localizedName } from "@/lib/names";
import { pinErrors } from "@/lib/pin";
import { useErrorText, useFieldError } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { DuplicateRoleDialog, RoleDialog } from "./RoleDialog";

type Errors = Partial<Record<string, string>>;

export function UsersSettings({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const users = useUsers();
  const roles = useRoles();
  const update = useUpdateUser();
  const deactivate = useDeactivateUser();
  const removeRole = useDeleteRole();
  const [editingUser, setEditingUser] = useState<User | "new" | null>(null);
  const [pinUser, setPinUser] = useState<User | null>(null);
  const [editingRole, setEditingRole] = useState<Role | "new" | null>(null);
  // USR-015: a new user or role on the model of another
  const [copyingUser, setCopyingUser] = useState<User | null>(null);
  const [copyingRole, setCopyingRole] = useState<Role | null>(null);
  // Only the owner creates, edits and removes roles (spec §4.5); the commands refuse anyone else.
  const canEditRoles = session.isOwner;
  const failure = update.error ?? deactivate.error ?? removeRole.error;

  return (
    <div className="flex flex-col gap-6">
      <Card>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2 className="text-xl font-bold">{t("settings:users.title")}</h2>
          <Button variant="primary" onClick={() => setEditingUser("new")}>
            <Plus aria-hidden />
            {t("settings:users.add")}
          </Button>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full border-collapse">
            <thead className="sticky top-0 bg-surface text-sm text-ink-2">
              <tr className="h-10">
                <th className="text-start font-semibold">{t("settings:users.columns.name")}</th>
                <th className="text-start font-semibold">{t("settings:users.columns.role")}</th>
                <th className="text-start font-semibold max-xl:hidden">
                  {t("settings:users.columns.language")}
                </th>
                <th className="text-start font-semibold">{t("settings:users.columns.status")}</th>
                <th className="text-start font-semibold">{t("settings:users.columns.lastLogin")}</th>
                <th className="text-end font-semibold">{t("settings:users.columns.actions")}</th>
              </tr>
            </thead>
            <tbody>
              {users.data?.map((u) => (
                <tr key={u.id} className="h-12 border-t border-line even:bg-surface-2">
                  <td>
                    <span className="flex items-center gap-2">
                      <Avatar name={u.displayName} />
                      <span className="font-semibold">{u.displayName}</span>
                      {u.id === session.userId ? (
                        <span className="text-ink-2">{t("common:status.you")}</span>
                      ) : null}
                    </span>
                  </td>
                  <td>
                    {localizedName(
                      { nameAr: u.roleNameAr, nameFr: u.roleNameFr, nameEn: u.roleNameEn },
                      lang,
                    )}
                  </td>
                  <td className="max-xl:hidden">{t(`common:languages.${u.lang}`)}</td>
                  <td>
                    <Badge tone={u.active ? "ok" : "neutral"}>
                      {t(u.active ? "common:status.active" : "common:status.inactive")}
                    </Badge>
                  </td>
                  <td>
                    {u.lastLoginAt === null ? (
                      t("common:status.never")
                    ) : (
                      <bdi dir="ltr">{formatDate(u.lastLoginAt, lang, { withTime: true })}</bdi>
                    )}
                  </td>
                  <td>
                    {/* Icon buttons, named by aria-label and title, and no language column below
                        xl: labelled buttons pushed the actions out of view below ~1300 px (the
                        reference PC is 1024×768). */}
                    <div className="flex justify-end gap-1">
                      <Button
                        variant="ghost"
                        size="icon"
                        aria-label={t("common:actions.edit")}
                        title={t("common:actions.edit")}
                        onClick={() => setEditingUser(u)}
                      >
                        <Pencil aria-hidden />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        aria-label={t("settings:users.changePin")}
                        title={t("settings:users.changePin")}
                        onClick={() => setPinUser(u)}
                      >
                        <KeyRound aria-hidden />
                      </Button>
                      {/* the owner is never a model: there is one owner */}
                      {u.isOwner ? null : (
                        <Button
                          variant="ghost"
                          size="icon"
                          aria-label={t("config:users.duplicate")}
                          title={t("config:users.duplicate")}
                          onClick={() => setCopyingUser(u)}
                        >
                          <Copy aria-hidden />
                        </Button>
                      )}
                      {u.active ? (
                        <Button
                          variant="danger"
                          size="icon"
                          aria-label={t("settings:users.deactivate")}
                          title={t("settings:users.deactivate")}
                          disabled={u.id === session.userId}
                          onClick={() => deactivate.mutate(u.id)}
                        >
                          <UserX aria-hidden />
                        </Button>
                      ) : (
                        <Button
                          variant="outline"
                          size="icon"
                          aria-label={t("settings:users.reactivate")}
                          title={t("settings:users.reactivate")}
                          onClick={() =>
                            update.mutate({
                              id: u.id,
                              input: {
                                displayName: u.displayName,
                                roleId: u.roleId,
                                lang: u.lang,
                                discountLimitBp: u.discountLimitBp,
                                active: true,
                              },
                            })
                          }
                        >
                          <UserCheck aria-hidden />
                        </Button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Card>

      <AutoLockCard />

      <Card>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2 className="text-xl font-bold">{t("settings:roles.title")}</h2>
          {canEditRoles ? (
            <Button variant="outline" onClick={() => setEditingRole("new")}>
              <Plus aria-hidden />
              {t("settings:roles.add")}
            </Button>
          ) : null}
        </div>
        <ul className="flex flex-col divide-y divide-line">
          {roles.data?.map((r) => (
            <li key={r.id} className="flex flex-wrap items-center gap-3 py-3">
              <span className="font-semibold">{localizedName(r, lang)}</span>
              {r.isSystem ? <Badge>{t("settings:roles.system")}</Badge> : null}
              <span className="text-ink-2">
                {t("settings:roles.usersSummary", {
                  users: t("settings:roles.usersCount", { count: r.userCount }),
                  active: t("settings:roles.activeCount", { count: r.activeUserCount }),
                })}
              </span>
              {canEditRoles ? (
                <div className="ms-auto flex gap-1">
                  <Button variant="ghost" onClick={() => setEditingRole(r)}>
                    <Pencil aria-hidden />
                    {t("common:actions.edit")}
                  </Button>
                  {r.isOwner ? null : (
                    <Button variant="ghost" onClick={() => setCopyingRole(r)}>
                      <Copy aria-hidden />
                      {t("config:roles.duplicate")}
                    </Button>
                  )}
                  {r.isSystem ? null : (
                    <Button
                      variant="danger"
                      onClick={() =>
                        removeRole.mutate(r.id, {
                          onSuccess: () => toast.success(t("settings:roles.removed")),
                        })
                      }
                    >
                      {t("settings:roles.remove")}
                    </Button>
                  )}
                </div>
              ) : null}
            </li>
          ))}
        </ul>
      </Card>

      {failure ? (
        <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
          {errorText(failure)}
        </p>
      ) : null}
      {editingUser ? (
        <UserDialog
          user={editingUser === "new" ? null : editingUser}
          roles={roles.data ?? []}
          onClose={() => setEditingUser(null)}
        />
      ) : null}
      {pinUser ? <PinDialog user={pinUser} onClose={() => setPinUser(null)} /> : null}
      {editingRole ? (
        <RoleDialog role={editingRole === "new" ? null : editingRole} onClose={() => setEditingRole(null)} />
      ) : null}
      {copyingUser ? <DuplicateUserDialog user={copyingUser} onClose={() => setCopyingUser(null)} /> : null}
      {copyingRole ? <DuplicateRoleDialog role={copyingRole} onClose={() => setCopyingRole(null)} /> : null}
    </div>
  );
}

function UserDialog({ user, roles, onClose }: { user: User | null; roles: Role[]; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const create = useCreateUser();
  const update = useUpdateUser();
  const [displayName, setDisplayName] = useState(user?.displayName ?? "");
  const [username, setUsername] = useState("");
  const [roleId, setRoleId] = useState(
    user?.roleId ?? roles.find((r) => r.code === "cashier")?.id ?? roles[0]?.id ?? "",
  );
  const [userLang, setUserLang] = useState<Lang>(user?.lang ?? lang);
  const [discount, setDiscount] = useState(formatPercentInput(user?.discountLimitBp ?? 0, lang));
  const [pin, setPin] = useState("");
  const [pinConfirm, setPinConfirm] = useState("");
  const [errors, setErrors] = useState<Errors>({});
  const mutation = user ? update : create;
  const errorOf = useFieldError(errors);

  const save = () => {
    const bp = parsePercentBp(discount);
    const found: Errors = user ? {} : { ...pinErrors(pin, pinConfirm) };
    if (!displayName.trim()) found.displayName = "validation:required";
    if (bp === null) found.discount = "validation:percentRange";
    setErrors(found);
    if (Object.keys(found).length > 0 || bp === null) return;
    const base = { displayName: displayName.trim(), roleId, lang: userLang, discountLimitBp: bp };
    if (user) {
      update.mutate(
        { id: user.id, input: { ...base, active: user.active } },
        {
          onSuccess: () => {
            toast.success(t("settings:users.updated"));
            onClose();
          },
        },
      );
    } else {
      create.mutate(
        { ...base, username: username.trim() === "" ? null : username.trim(), pin },
        {
          onSuccess: () => {
            toast.success(t("settings:users.created"));
            onClose();
          },
        },
      );
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t(user ? "settings:users.edit" : "settings:users.add")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" onClick={save} disabled={mutation.isPending}>
            {t("settings:users.save")}
          </Button>
        </>
      }
    >
      <Field id="user-name" label={t("settings:users.displayName")} error={errorOf("displayName")}>
        <Input
          id="user-name"
          dir="auto"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
        />
      </Field>
      {user ? null : (
        <Field id="user-username" label={t("settings:users.username")} optional={t("setup:shop.optional")}>
          <Input
            id="user-username"
            dir="ltr"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
          />
        </Field>
      )}
      <div className="grid gap-4 sm:grid-cols-2">
        <Field id="user-role" label={t("settings:users.role")}>
          <NativeSelect id="user-role" value={roleId} onChange={(e) => setRoleId(e.target.value)}>
            {roles.map((r) => (
              <option key={r.id} value={r.id}>
                {localizedName(r, lang)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id="user-lang" label={t("common:languages.label")}>
          <NativeSelect
            id="user-lang"
            value={userLang}
            onChange={(e) => {
              if (isLang(e.target.value)) setUserLang(e.target.value);
            }}
          >
            {LANGS.map((l) => (
              <option key={l} value={l}>
                {t(`common:languages.${l}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
      </div>
      <Field id="user-discount" label={t("settings:users.discountLimit")} error={errorOf("discount")}>
        <Input
          id="user-discount"
          dir="ltr"
          inputMode="decimal"
          value={discount}
          onChange={(e) => setDiscount(e.target.value)}
        />
      </Field>
      {user ? null : (
        <div className="grid gap-4 sm:grid-cols-2">
          <Field id="user-pin" label={t("settings:users.pin")} error={errorOf("pin")}>
            <PinInput id="user-pin" value={pin} onChange={setPin} />
          </Field>
          <Field id="user-pin-confirm" label={t("settings:users.pinConfirm")} error={errorOf("pinConfirm")}>
            <PinInput id="user-pin-confirm" value={pinConfirm} onChange={setPinConfirm} />
          </Field>
        </div>
      )}
      {mutation.error ? (
        <p role="alert" className="text-danger">
          {errorText(mutation.error)}
        </p>
      ) : null}
    </Dialog>
  );
}

/** USR-015: a new user with the role, language and discount limit of `user`, their own name and
 * PIN (hashed by the backend like any PIN). */
function DuplicateUserDialog({ user, onClose }: { user: User; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const duplicate = useDuplicateUser();
  const [displayName, setDisplayName] = useState("");
  const [pin, setPin] = useState("");
  const [confirm, setConfirm] = useState("");
  const [errors, setErrors] = useState<Errors>({});
  const errorOf = useFieldError(errors);
  const save = () => {
    const found: Errors = { ...pinErrors(pin, confirm) };
    if (!displayName.trim()) found.displayName = "validation:required";
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    duplicate.mutate(
      { templateId: user.id, displayName: displayName.trim(), pin },
      {
        onSuccess: () => {
          toast.success(t("settings:users.created"));
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
      title={t("config:users.duplicateTitle", { name: user.displayName })}
      description={t("config:users.duplicateHint", { name: user.displayName })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" onClick={save} disabled={duplicate.isPending}>
            {t("config:users.duplicateSave")}
          </Button>
        </>
      }
    >
      <Field id="user-copy-name" label={t("settings:users.displayName")} error={errorOf("displayName")}>
        <Input
          id="user-copy-name"
          dir="auto"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
        />
      </Field>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field id="user-copy-pin" label={t("settings:users.pin")} error={errorOf("pin")}>
          <PinInput id="user-copy-pin" value={pin} onChange={setPin} />
        </Field>
        <Field
          id="user-copy-pin-confirm"
          label={t("settings:users.pinConfirm")}
          error={errorOf("pinConfirm")}
        >
          <PinInput id="user-copy-pin-confirm" value={confirm} onChange={setConfirm} />
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

function PinDialog({ user, onClose }: { user: User; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const setUserPin = useSetUserPin();
  const [pin, setPin] = useState("");
  const [confirm, setConfirm] = useState("");
  const [errors, setErrors] = useState<Errors>({});
  const errorOf = useFieldError(errors);
  const save = () => {
    const found = pinErrors(pin, confirm);
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    setUserPin.mutate(
      { id: user.id, pin },
      {
        onSuccess: () => {
          toast.success(t("settings:users.pinChanged"));
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
      title={t("settings:users.changePin")}
      description={user.displayName}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" onClick={save} disabled={setUserPin.isPending}>
            {t("settings:users.changePin")}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 sm:grid-cols-2">
        <Field id="change-pin" label={t("settings:users.pin")} error={errorOf("pin")}>
          <PinInput id="change-pin" value={pin} onChange={setPin} />
        </Field>
        <Field id="change-pin-confirm" label={t("settings:users.pinConfirm")} error={errorOf("pinConfirm")}>
          <PinInput id="change-pin-confirm" value={confirm} onChange={setConfirm} />
        </Field>
      </div>
      {setUserPin.error ? (
        <p role="alert" className="text-danger">
          {errorText(setUserPin.error)}
        </p>
      ) : null}
    </Dialog>
  );
}

function AutoLockCard() {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const current = useAutoLockMinutes(true);
  const set = useSetSetting();
  const [typed, setTyped] = useState<string | null>(null);
  const shown = typed ?? String(current);
  const minutes = Number(shown);
  const valid = /^\d{1,3}$/.test(shown) && minutes <= 240;
  return (
    <Card>
      <Field
        id="auto-lock"
        label={t("settings:users.autoLock")}
        error={valid ? undefined : t("validation:minutesRange")}
      >
        <Input
          id="auto-lock"
          dir="ltr"
          inputMode="numeric"
          className="max-w-40"
          value={shown}
          onChange={(e) => setTyped(e.target.value)}
        />
      </Field>
      {set.error ? (
        <p role="alert" className="text-danger">
          {errorText(set.error)}
        </p>
      ) : null}
      <div>
        <Button
          variant="outline"
          disabled={!valid || set.isPending}
          onClick={() =>
            set.mutate(
              { key: "security.auto_lock_minutes", value: minutes },
              { onSuccess: () => toast.success(t("settings:users.autoLockSaved")) },
            )
          }
        >
          {t("settings:users.saveAutoLock")}
        </Button>
      </div>
    </Card>
  );
}
