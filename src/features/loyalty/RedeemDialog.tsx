import { useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { formatDZD } from "@/lib/format";
import { useRedeemPoints } from "@/lib/ipc/loyalty";
import { useSetting } from "@/lib/ipc/settings";
import type { Account, VoucherDone } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { VoucherIssued } from "./VoucherIssued";

/**
 * LOY-002 lite: points turned into a loyalty voucher, written by the main PC and printed here.
 * The value line multiplies the points by the account's point value as a preview only: the
 * backend computes the voucher's amount again. A lazy component on purpose: the till mounts it
 * from its pack chunk (Task 24) with the back office's texts loaded.
 */
export function RedeemDialog({
  account,
  onClose,
  onRedeemed,
}: {
  account: Account;
  onClose: () => void;
  onRedeemed?: (done: VoucherDone) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const redeem = useRedeemPoints();
  const minimum = useSetting("loyalty.min_redeem_points").data;
  const min = typeof minimum === "number" && minimum > 0 ? minimum : 1;
  const [text, setText] = useState(String(account.balance));
  const [done, setDone] = useState<VoucherDone | null>(null);
  const points = /^\d+$/.test(text) ? Number(text) : null;
  const valid = points !== null && Number.isSafeInteger(points) && points >= min && points <= account.balance;
  const value = valid ? points * account.pointValue : null;
  const submit = () => {
    if (!valid) return;
    redeem.mutate(
      { customerId: account.customerId, points },
      {
        onSuccess: (result) => {
          setDone(result);
          onRedeemed?.(result);
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("loyalty:redeem.title")}
      footer={
        done ? (
          <Button variant="primary" onClick={onClose}>
            {t("common:actions.done")}
          </Button>
        ) : (
          <>
            <Button variant="outline" onClick={onClose}>
              {t("common:actions.cancel")}
            </Button>
            <Button variant="primary" disabled={!valid || redeem.isPending} onClick={submit}>
              {t("loyalty:redeem.submit")}
            </Button>
          </>
        )
      }
    >
      {done ? (
        <VoucherIssued
          done={done}
          messageKey="loyalty:redeem.done"
          notPrintedKey="loyalty:redeem.notPrinted"
        />
      ) : (
        <form
          noValidate
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <Field
            id="redeem-points"
            label={t("loyalty:redeem.points")}
            hint={
              min > 1
                ? t("loyalty:redeem.range", { min, max: account.balance })
                : t("loyalty:redeem.max", { max: account.balance })
            }
          >
            <NumericInput
              id="redeem-points"
              kind="integer"
              autoFocus
              value={text}
              aria-invalid={text !== "" && !valid}
              onValueChange={setText}
              className="text-2xl font-bold"
            />
          </Field>
          {value !== null ? (
            <p aria-live="polite" className="rounded-card bg-surface-2 p-4 text-xl font-bold">
              {t("loyalty:redeem.value", { amount: formatDZD(value, lang) })}
            </p>
          ) : null}
          {redeem.error ? (
            <p role="alert" className="text-danger">
              {errorText(redeem.error)}
            </p>
          ) : null}
        </form>
      )}
    </Dialog>
  );
}
