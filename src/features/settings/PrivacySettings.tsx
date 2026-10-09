import { Printer } from "lucide-react";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { toast } from "sonner";
import { LanguageToggle } from "@/components/language-toggle";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import type { Lang } from "@/lib/i18n";
import { useSession } from "@/lib/ipc/auth";
import { useTillLink } from "@/lib/ipc/lan";
import { usePrintProcessingRegister } from "@/lib/ipc/privacy";
import { useSetSetting, useSetting } from "@/lib/ipc/settings";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

interface Dpo {
  name: string;
  email: string;
  phone: string;
}

/** The stored `privacy.dpo` (`null` or `{ name, email?, phone? }`) as the form's fields. */
function dpoOf(value: unknown): Dpo {
  const v = (value ?? {}) as Partial<Record<keyof Dpo, unknown>>;
  const text = (x: unknown) => (typeof x === "string" ? x : "");
  return { name: text(v.name), email: text(v.email), phone: text(v.phone) };
}

const NAME_MAX = 120;

/** Settings › Confidentialité (PDP-005): the data protection officer, the processing register
 * printed for the ANPDP, and the journal of the accesses to personal data (RPT-020). */
export function PrivacySettings() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const till = useTillLink();
  const permissions = useSession().data?.permissions ?? [];
  const stored = useSetting("privacy.dpo");
  const save = useSetSetting();
  const print = usePrintProcessingRegister();
  const [typed, setTyped] = useState<Partial<Dpo>>({});
  const [tried, setTried] = useState(false);
  const [printLang, setPrintLang] = useState<Lang>(lang);
  const shown = { ...dpoOf(stored.data), ...typed };
  const empty = Object.values(shown).every((v) => v.trim() === "");
  const nameOk = empty || (shown.name.trim() !== "" && [...shown.name.trim()].length <= NAME_MAX);
  const field = (key: keyof Dpo) => ({
    value: shown[key],
    onChange: (event: { target: { value: string } }) =>
      setTyped((current) => ({ ...current, [key]: event.target.value })),
  });
  const submit = (event: FormEvent) => {
    event.preventDefault();
    setTried(true);
    if (!nameOk) return;
    const optional = (v: string) => (v.trim() === "" ? null : v.trim());
    save.mutate(
      {
        key: "privacy.dpo",
        value: empty
          ? null
          : { name: shown.name.trim(), email: optional(shown.email), phone: optional(shown.phone) },
      },
      {
        onSuccess: () => {
          setTyped({});
          setTried(false);
          toast.success(t("privacy:dpo.saved"));
        },
      },
    );
  };
  return (
    <div className="flex flex-col gap-6">
      <HostOnlyNote />
      <Card aria-labelledby="dpo-title">
        <h2 id="dpo-title" className="text-xl font-bold">
          {t("privacy:dpo.title")}
        </h2>
        <p className="text-ink-2">{t("privacy:dpo.intro")}</p>
        <form noValidate onSubmit={submit} className="flex flex-col gap-4">
          <fieldset disabled={till.isTill} className="grid gap-4 sm:grid-cols-3">
            <Field
              id="dpo-name"
              label={t("privacy:dpo.name")}
              error={tried && !nameOk ? t("validation:required") : undefined}
            >
              <Input id="dpo-name" dir="auto" maxLength={NAME_MAX} {...field("name")} />
            </Field>
            <Field id="dpo-email" label={t("privacy:dpo.email")} optional={t("customers:form.optional")}>
              <Input id="dpo-email" type="email" dir="ltr" maxLength={120} {...field("email")} />
            </Field>
            <Field id="dpo-phone" label={t("privacy:dpo.phone")} optional={t("customers:form.optional")}>
              <Input id="dpo-phone" dir="ltr" inputMode="tel" maxLength={20} {...field("phone")} />
            </Field>
          </fieldset>
          {save.error ? (
            <p role="alert" className="text-danger">
              {errorText(save.error)}
            </p>
          ) : null}
          {till.isTill ? null : (
            <Button type="submit" variant="primary" className="self-start" disabled={save.isPending}>
              {t("privacy:dpo.save")}
            </Button>
          )}
        </form>
      </Card>
      <Card>
        <div className="flex flex-wrap items-center gap-3">
          <span className="font-semibold">{t("billing:print.language")}</span>
          <LanguageToggle value={printLang} onChange={setPrintLang} />
          <Button
            variant="secondary"
            disabled={print.isPending}
            onClick={() =>
              print.mutate(printLang, {
                onSuccess: () => toast.success(t("privacy:dpo.registerPrinted")),
                onError: (error) => toast.error(errorText(error)),
              })
            }
          >
            <Printer aria-hidden />
            {t("privacy:dpo.register")}
          </Button>
        </div>
        {permissions.includes("audit.view") ? (
          <Link
            to="/reports/personal_data_access"
            className="inline-flex min-h-11 items-center self-start font-semibold text-info hover:underline"
          >
            {t("privacy:dpo.accessReport")}
          </Link>
        ) : null}
      </Card>
    </div>
  );
}
