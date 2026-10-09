import { useQueryClient } from "@tanstack/react-query";
import { Globe, MessageCircle, Save } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { toAsciiDigits } from "@/lib/digits";
import { useOpenSupport, useSupportContact } from "@/lib/ipc/diagnostics";
import { qk } from "@/lib/ipc/keys";
import { useTillLink } from "@/lib/ipc/lan";
import { useSetSetting, useSetting } from "@/lib/ipc/settings";
import type { JsonValue, Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";

/** ONB-012: the shop's support (its reseller or technician) on WhatsApp or its web page — buttons
 * for everyone, opened by Rust in the browser; the contact itself is edited by `settings.company`
 * holders where `editable` (the Diagnostic screen, not About). Without a session (a help page in
 * the setup wizard) the contact answers `session_required` and the card shows nothing. */
export function SupportCard({
  session,
  editable = true,
  title,
}: {
  session: Session | null;
  editable?: boolean;
  /** The heading, "Assistance" by default (a help page asks "Besoin d'aide ?"). */
  title?: string;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const contact = useSupportContact();
  const open = useOpenSupport();
  const canEdit = editable && session !== null && session.permissions.includes("settings.company");
  const whatsapp = contact.data?.whatsapp ?? null;
  const url = contact.data?.url ?? null;
  if (!canEdit && whatsapp === null && url === null) return null;
  const name = contact.data?.label || t("settings:support.defaultName");
  return (
    <Card aria-labelledby="support-title">
      <h2 id="support-title" className="text-xl font-bold">
        {title ?? t("settings:support.title")}
      </h2>
      {whatsapp !== null || url !== null ? (
        <div className="flex flex-wrap gap-2">
          {whatsapp !== null ? (
            <Button variant="outline" disabled={open.isPending} onClick={() => open.mutate("whatsapp")}>
              <MessageCircle aria-hidden />
              {t("settings:support.whatsapp", { name })}
            </Button>
          ) : null}
          {url !== null ? (
            <Button variant="outline" disabled={open.isPending} onClick={() => open.mutate("web")}>
              <Globe aria-hidden />
              {t("settings:support.web")}
            </Button>
          ) : null}
        </div>
      ) : null}
      {open.error ? (
        <p role="alert" className="text-danger">
          {errorText(open.error)}
        </p>
      ) : null}
      {canEdit ? <ContactEditor /> : null}
    </Card>
  );
}

/** The support contact is the shop's (a shared setting, the main PC's); read-only on a
 * till, with the note naming the main PC. */
function ContactEditor() {
  const till = useTillLink();
  return (
    <>
      <HostOnlyNote />
      <fieldset disabled={till.isTill} className="contents">
        <ContactLoader />
      </fieldset>
    </>
  );
}

interface ContactDraft {
  label: string;
  whatsapp: string;
  url: string;
}

function draftOf(value: JsonValue | undefined): ContactDraft {
  const saved = value !== null && typeof value === "object" && !Array.isArray(value) ? value : {};
  const text = (key: string) => {
    const field = saved[key];
    return typeof field === "string" ? field : "";
  };
  return { label: text("label"), whatsapp: text("whatsapp"), url: text("url") };
}

/** The form starts from the setting as saved, once it loaded. */
function ContactLoader() {
  const setting = useSetting("support.contact");
  if (setting.isPending) return null;
  return <ContactForm initial={draftOf(setting.data)} />;
}

function ContactForm({ initial }: { initial: ContactDraft }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const client = useQueryClient();
  const set = useSetSetting();
  const [draft, setDraft] = useState(initial);
  const [saved, setSaved] = useState(false);
  const change = (key: keyof ContactDraft) => (event: { target: { value: string } }) => {
    setSaved(false);
    setDraft({ ...draft, [key]: event.target.value });
  };
  const save = () => {
    // The setting takes international digits only (`213…`): Arabic-Indic digits, spaces and a
    // leading "+" typed by habit are cleaned; anything else is the backend's `value_invalid`.
    const whatsapp = toAsciiDigits(draft.whatsapp).replace(/\s+/g, "").replace(/^\+/, "");
    const value: Record<string, string> = {};
    if (draft.label.trim() !== "") value.label = draft.label.trim();
    if (whatsapp !== "") value.whatsapp = whatsapp;
    if (draft.url.trim() !== "") value.url = draft.url.trim();
    set.mutate(
      { key: "support.contact", value },
      {
        onSuccess: () => {
          setSaved(true);
          void client.invalidateQueries({ queryKey: qk.support });
        },
      },
    );
  };
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        save();
      }}
    >
      <Field id="support-label" label={t("settings:support.label")}>
        <Input id="support-label" maxLength={60} value={draft.label} onChange={change("label")} />
      </Field>
      <Field id="support-whatsapp" label={t("settings:support.number")}>
        <Input
          id="support-whatsapp"
          dir="ltr"
          inputMode="tel"
          value={draft.whatsapp}
          onChange={change("whatsapp")}
        />
      </Field>
      <Field id="support-url" label={t("settings:support.url")}>
        <Input id="support-url" dir="ltr" inputMode="url" value={draft.url} onChange={change("url")} />
      </Field>
      {set.error ? (
        <p role="alert" className="text-danger">
          {errorText(set.error)}
        </p>
      ) : null}
      {saved ? (
        <p role="status" className="text-ok">
          {t("settings:support.saved")}
        </p>
      ) : null}
      <div>
        <Button type="submit" variant="outline" disabled={set.isPending}>
          <Save aria-hidden />
          {t("settings:support.save")}
        </Button>
      </div>
    </form>
  );
}
