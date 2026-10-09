import { FileKey } from "lucide-react";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { useActivateLicense, useImportLicenseFile } from "@/lib/ipc/license";
import type { LicenseStatus } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";

/** Paste the key (several lines are fine: Rust keeps only the key) or load the `.dzlic` through
 * Rust's dialog. Rust checks everything; the screen only says what it answered. */
export function ActivationForm({ onActivated }: { onActivated?: (status: LicenseStatus) => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const activate = useActivateLicense();
  const importFile = useImportLicenseFile();
  const [key, setKey] = useState("");
  const busy = activate.isPending || importFile.isPending;
  const error = activate.error ?? importFile.error;
  const done = (status: LicenseStatus) => {
    toast.success(t("license:form.done"));
    setKey("");
    onActivated?.(status);
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const text = key.trim();
    if (!text) return;
    importFile.reset();
    activate.mutate(text, { onSuccess: done });
  };
  return (
    <form noValidate onSubmit={submit} className="flex flex-col gap-4">
      <Field id="license-key" label={t("license:form.key")} hint={t("license:form.keyHint")}>
        <textarea
          id="license-key"
          dir="ltr"
          rows={3}
          spellCheck={false}
          autoComplete="off"
          value={key}
          onChange={(event) => setKey(event.target.value)}
          className="w-full rounded-control border border-line bg-surface px-3 py-2 text-base break-all text-ink"
        />
      </Field>
      {error ? (
        <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
          {errorText(error)}
        </p>
      ) : null}
      <div className="flex flex-wrap gap-3">
        <Button type="submit" variant="primary" size="lg" disabled={busy || !key.trim()}>
          {t(activate.isPending ? "license:form.activating" : "license:form.activate")}
        </Button>
        <Button
          type="button"
          variant="outline"
          size="lg"
          disabled={busy}
          onClick={() => {
            activate.reset();
            importFile.mutate(undefined, {
              onSuccess: (status) => {
                if (status) done(status);
              },
            });
          }}
        >
          <FileKey aria-hidden />
          {t("license:form.file")}
        </Button>
      </div>
    </form>
  );
}
