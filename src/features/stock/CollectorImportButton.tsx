import { FileUp } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { isApprovalCancelled, useApproval } from "@/components/manager-approval";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { useImportCollector } from "@/lib/ipc/labels";
import type { CollectorImport } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";

/**
 * STK-012: a data collector's `code;quantity` file, picked in a Rust open dialog, added to the open
 * count `countId` (a manager's approval counts at the till). The result says how many lines were
 * counted, which codes name no product and which lines could not be read.
 */
export function CollectorImportButton({ countId }: { countId: string }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const approval = useApproval();
  const importer = useImportCollector();
  const [result, setResult] = useState<CollectorImport | null>(null);

  const run = () => {
    approval
      .run(() => importer.mutateAsync(countId))
      .then((done) => {
        // `null`: the open dialog was cancelled
        if (done) setResult(done);
      })
      .catch((error: unknown) => {
        if (!isApprovalCancelled(error)) toast.error(errorText(error));
      });
  };

  return (
    <>
      <Button variant="outline" disabled={importer.isPending || approval.pending} onClick={run}>
        <FileUp aria-hidden />
        {t("stock:collector.import")}
      </Button>
      {approval.dialog}
      {result ? (
        <Dialog
          open
          onOpenChange={(open) => (open ? undefined : setResult(null))}
          title={t("stock:collector.title")}
          footer={
            <Button variant="primary" onClick={() => setResult(null)}>
              {t("common:actions.done")}
            </Button>
          }
        >
          <p role="status" className="text-lg font-semibold text-ok">
            {t("stock:collector.result", { count: result.matched })}
          </p>
          {result.unknown.length > 0 ? (
            <section aria-labelledby="collector-unknown" className="flex flex-col gap-2">
              <h3 id="collector-unknown" className="font-semibold">
                {t("stock:collector.unknown")}
              </h3>
              <p className="text-sm text-ink-2">{t("stock:collector.unknownHint")}</p>
              <ul className="flex flex-wrap gap-2">
                {result.unknown.map((code) => (
                  <li key={code} className="rounded-control bg-surface-2 px-2 py-1 tabular-nums">
                    <bdi dir="ltr">{code}</bdi>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
          {result.problems.length > 0 ? (
            <section aria-labelledby="collector-problems" className="flex flex-col gap-2">
              <h3 id="collector-problems" className="font-semibold">
                {t("stock:collector.problemsTitle")}
              </h3>
              <ul className="flex flex-col gap-1 text-danger">
                {result.problems.map((problem) => (
                  <li key={`${problem.line}:${problem.code}`}>
                    {t(`stock:collector.problems.${problem.code}`, { line: problem.line })}
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
        </Dialog>
      ) : null}
    </>
  );
}
