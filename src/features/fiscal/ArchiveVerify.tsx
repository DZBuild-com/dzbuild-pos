import { ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { useVerifyArchive } from "@/lib/ipc/fiscal";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** Checks an archive picked in the Rust open dialog against its manifest (AUD-011): "intact", or
 * the files changed since the export. A cancelled dialog shows nothing. */
export function ArchiveVerify() {
  const { t } = useTranslation("fiscal");
  const lang = useLang();
  const errorText = useErrorText();
  const verify = useVerifyArchive();
  const result = verify.data;
  return (
    <Card>
      <div>
        <Button variant="outline" disabled={verify.isPending} onClick={() => verify.mutate()}>
          <ShieldCheck aria-hidden />
          {t("archive.verify")}
        </Button>
      </div>
      {result && result.badFiles.length === 0 ? (
        <p role="status" className="text-ok">
          {t("archive.verifyOk")}
        </p>
      ) : null}
      {result && result.badFiles.length > 0 ? (
        <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
          {t("archive.verifyBad", {
            files: new Intl.ListFormat(lang, { type: "conjunction" }).format(result.badFiles),
          })}
        </p>
      ) : null}
      {verify.isError ? (
        <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
          {errorText(verify.error)}
        </p>
      ) : null}
    </Card>
  );
}
