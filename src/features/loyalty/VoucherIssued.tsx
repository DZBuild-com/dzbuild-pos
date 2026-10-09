import { Printer } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { Button } from "@/components/ui/button";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { useReprintVoucher } from "@/lib/ipc/loyalty";
import type { VoucherDone } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** A voucher just written: its code, amount and expiry, whether its slip went to the printer, and
 * "Réimprimer" for whoever may reprint (`vouchers_reprint`). */
export function VoucherIssued({
  done,
  messageKey,
  notPrintedKey,
}: {
  done: VoucherDone;
  messageKey: "loyalty:redeem.done" | "loyalty:sell.done";
  notPrintedKey: "loyalty:redeem.notPrinted" | "loyalty:sell.notPrinted";
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const reprint = useReprintVoucher();
  const canReprint = useSession().data?.permissions.includes("sales.reprint") ?? false;
  const { voucher } = done;
  return (
    <div role="status" className="flex flex-col gap-3 rounded-card bg-ok-soft p-4">
      <p className="font-semibold text-ok">
        {t(messageKey, { code: voucher.code, date: formatDate(dayToMs(voucher.expiresOn), lang) })}
      </p>
      <p className="flex flex-wrap items-center gap-3">
        <bdi dir="ltr" className="text-xl font-bold">
          {voucher.code}
        </bdi>
        <Amount value={voucher.initialAmount} className="text-xl font-bold" />
      </p>
      {done.printQueued ? null : <p className="text-warn">{t(notPrintedKey)}</p>}
      {canReprint ? (
        <Button
          variant="outline"
          className="self-start"
          disabled={reprint.isPending}
          onClick={() =>
            reprint.mutate(voucher.code, {
              onSuccess: () => toast.success(t("loyalty:list.reprinted", { code: voucher.code })),
              onError: (error) => toast.error(errorText(error)),
            })
          }
        >
          <Printer aria-hidden />
          {t("loyalty:list.reprint")}
        </Button>
      ) : null}
    </div>
  );
}
