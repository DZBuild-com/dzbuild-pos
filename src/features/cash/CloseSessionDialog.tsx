import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { useApproval } from "@/components/manager-approval";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { UpdateAfterClose } from "@/features/updates/UpdateAfterClose";
import { isApprovalCancelled } from "@/lib/approval";
import { useCloseOffline, useCloseSession, usePrintZ, useXReport } from "@/lib/ipc/cash";
import { toAppError } from "@/lib/ipc/client";
import { useSyncNow } from "@/lib/ipc/lan";
import type { Closed, Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { CountSheet } from "./CountSheet";
import { type Counts, countLines, emptyCounts } from "./denominations";
import { ReportView } from "./ReportView";

type Step = "count" | "recount";

/** CSH-003/004/005/007: count the drawer, recount when the variance is above the threshold, then a
 * manager's approval; the Z is shown and printed, then a downloaded update is offered (PLT-011).
 * Expected cash appears only when the backend reveals it (cash.view_expected and blind close off).
 * On a till whose operations have not all reached the main PC, the cashier waits for them or
 * closes an offline Z with a manager's approval (M5). */
export function CloseSessionDialog({ session, onClose }: { session: Session; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const x = useXReport(true);
  const close = useCloseSession();
  const closeOffline = useCloseOffline();
  const syncNow = useSyncNow();
  const printZ = usePrintZ();
  const approval = useApproval();
  const [step, setStep] = useState<Step>("count");
  const [counts, setCounts] = useState<Counts>(emptyCounts);
  const [notes, setNotes] = useState("");
  const [closed, setClosed] = useState<Closed | null>(null);
  const [error, setError] = useState<unknown>(null);
  // the operations of this till not yet on the main PC when the close was refused
  const [offline, setOffline] = useState<number | null>(null);

  const print = (done: Closed) => printZ.mutate({ sessionId: done.z.summary.session.id, lang });

  const submit = async () => {
    setError(null);
    const input = { counts: countLines(counts), notes: notes.trim() === "" ? null : notes.trim() };
    try {
      const done =
        step === "recount"
          ? await approval.run(() => close.mutateAsync(input))
          : await close.mutateAsync(input);
      setClosed(done);
      print(done);
    } catch (e) {
      const refused = toAppError(e);
      if (refused.code === "outbox_pending_at_close") {
        const pending = (refused.details as { pending?: unknown } | null | undefined)?.pending;
        setOffline(typeof pending === "number" ? pending : 0);
        return;
      }
      if (refused.code === "variance_needs_approval" && step === "count") {
        setStep("recount");
        setCounts(emptyCounts());
        return;
      }
      if (!isApprovalCancelled(e)) setError(e);
    }
  };

  // CSH-005 offline too: a first count over the threshold is refused and counted again before any
  // manager is asked; the manager then approves the recount (and the offline Z) knowingly
  const submitOffline = async () => {
    setError(null);
    const input = { counts: countLines(counts), notes: notes.trim() === "" ? null : notes.trim() };
    const attempt = () => closeOffline.mutateAsync(input);
    try {
      let done: Closed;
      if (step === "count") {
        try {
          done = await attempt();
        } catch (e) {
          const refused = toAppError(e);
          if (refused.code === "variance_needs_approval") {
            setStep("recount");
            setCounts(emptyCounts());
            return;
          }
          if (refused.code !== "offline_z_needs_approval") throw e;
          done = await approval.run(attempt);
        }
      } else {
        done = await approval.run(attempt);
      }
      setOffline(null);
      setClosed(done);
      print(done);
    } catch (e) {
      if (!isApprovalCancelled(e)) setError(e);
    }
  };

  if (closed) {
    return (
      <Dialog
        open
        size="lg"
        onOpenChange={(open) => (open ? undefined : onClose())}
        title={t("cash:close.done")}
        footer={
          <Button variant="primary" onClick={onClose}>
            {t("common:actions.done")}
          </Button>
        }
      >
        <ReportView
          summary={closed.z.summary}
          z={closed.z}
          onPrint={() => print(closed)}
          printing={printZ.isPending}
        />
        {printZ.isSuccess ? (
          <p role="status" className="text-ok">
            {t("cash:close.zPrinted")}
          </p>
        ) : null}
        {printZ.error ? (
          <p role="alert" className="text-danger">
            {errorText(printZ.error)}
          </p>
        ) : null}
        {closed.backupError ? (
          <p role="alert" className="text-warn">
            {t("cash:close.backupFailed")}
          </p>
        ) : null}
        <UpdateAfterClose session={session} />
      </Dialog>
    );
  }

  const expected = x.data?.expectedCash ?? null;
  return (
    <>
      <Dialog
        open={!approval.pending}
        size="lg"
        onOpenChange={(open) => (open ? undefined : onClose())}
        title={t("cash:close.title")}
        description={t("cash:close.hint")}
        footer={
          <>
            <Button variant="outline" onClick={onClose}>
              {t("common:actions.cancel")}
            </Button>
            {/* spec §10.4, one primary per screen: while the offline Z panel shows, its action is the
                primary and the close stays offered for when the operations have arrived */}
            <Button
              variant={offline !== null ? "outline" : "primary"}
              onClick={() => void submit()}
              disabled={close.isPending}
            >
              {t("cash:close.submit")}
            </Button>
          </>
        }
      >
        {step === "recount" ? (
          <p role="alert" className="rounded-control bg-warn-soft p-3 font-semibold text-warn">
            {t("cash:close.recount")}
          </p>
        ) : null}
        {offline !== null ? (
          <div role="alert" className="flex flex-col gap-2 rounded-control bg-warn-soft p-3">
            <p className="font-semibold">{t("sync:lan.offlineZ.title")}</p>
            <p>{t("sync:lan.offlineZ.body", { count: offline })}</p>
            <div className="flex flex-wrap gap-2">
              <Button
                variant="outline"
                onClick={() => {
                  setOffline(null);
                  syncNow.mutate();
                }}
              >
                {t("sync:lan.offlineZ.wait")}
              </Button>
              <Button
                variant="primary"
                disabled={closeOffline.isPending}
                onClick={() => void submitOffline()}
              >
                {t("sync:lan.offlineZ.approve")}
              </Button>
            </div>
          </div>
        ) : null}
        {expected !== null ? (
          <p className="flex justify-between text-lg">
            <span>{t("cash:report.expected")}</span>
            <Amount value={expected} className="font-bold" />
          </p>
        ) : null}
        <CountSheet counts={counts} onChange={setCounts} />
        <Field id="close-notes" label={t("cash:close.notes")} optional={t("setup:shop.optional")}>
          <Input id="close-notes" value={notes} maxLength={300} onChange={(e) => setNotes(e.target.value)} />
        </Field>
        {error ? (
          <p role="alert" className="text-danger">
            {errorText(error)}
          </p>
        ) : null}
      </Dialog>
      {approval.dialog}
    </>
  );
}
