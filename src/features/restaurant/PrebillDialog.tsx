import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { usePrebill } from "@/lib/ipc/restaurant";
import type { PreBill } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { cn } from "@/lib/utils";

const PARTS = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] as const;

/** RST-007: prints the addition ("ce document n'est pas un ticket de caisse"), shared in 1 to 10
 * equal parts; every figure (the total, the cash amount with its droit de timbre, the shares) is
 * the backend's. The table shows "addition imprimée" until a line changes. */
export function PrebillDialog({ ticketId, onClose }: { ticketId: string; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const prebill = usePrebill();
  const [parts, setParts] = useState(1);
  const [bill, setBill] = useState<PreBill | null>(null);
  const print = () =>
    prebill.mutate({ ticketId, parts: parts > 1 ? parts : null }, { onSuccess: (result) => setBill(result) });
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("restaurant:prebill.title")}
      footer={
        bill ? (
          <Button variant="primary" size="lg" onClick={onClose}>
            {t("common:actions.close")}
          </Button>
        ) : (
          <>
            <Button variant="outline" size="lg" onClick={onClose}>
              {t("common:actions.cancel")}
            </Button>
            <Button variant="primary" size="lg" disabled={prebill.isPending} onClick={print}>
              {t("restaurant:prebill.print")}
            </Button>
          </>
        )
      }
    >
      {bill ? (
        <dl className="grid gap-2 text-lg">
          <div className="flex justify-between gap-3">
            <dt>{t("restaurant:prebill.total")}</dt>
            <dd>
              <Amount value={bill.priced.totals.totalTtc} className="font-bold" />
            </dd>
          </div>
          {bill.stampDuty !== 0 ? (
            <div className="flex justify-between gap-3">
              <dt>{t("payments:stampDuty")}</dt>
              <dd>
                <Amount value={bill.stampDuty} />
              </dd>
            </div>
          ) : null}
          <div className="flex justify-between gap-3 font-bold">
            <dt>{t("restaurant:prebill.cashDue")}</dt>
            <dd>
              <Amount value={bill.cashDue} />
            </dd>
          </div>
          {bill.shares.map((share, index) => (
            <div key={index} className="flex justify-between gap-3 text-ink-2">
              <dt>{t("restaurant:prebill.share", { number: index + 1 })}</dt>
              <dd>
                <Amount value={share} />
              </dd>
            </div>
          ))}
        </dl>
      ) : (
        <div role="radiogroup" aria-label={t("restaurant:prebill.parts")} className="flex flex-col gap-2">
          <p className="font-semibold">{t("restaurant:prebill.parts")}</p>
          <div className="grid grid-cols-5 gap-2" dir="ltr">
            {PARTS.map((n) => (
              <button
                key={n}
                type="button"
                role="radio"
                aria-checked={parts === n}
                onClick={() => setParts(n)}
                className={cn(
                  "grid min-h-14 place-items-center rounded-control border-2 text-xl font-bold",
                  parts === n ? "border-brand bg-brand-soft" : "border-line bg-surface-2",
                )}
              >
                {n}
              </button>
            ))}
          </div>
        </div>
      )}
      {prebill.error ? (
        <p role="alert" className="text-danger">
          {errorText(prebill.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
