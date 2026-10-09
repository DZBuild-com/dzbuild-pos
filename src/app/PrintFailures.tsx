import { Printer } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Sheet } from "@/components/ui/sheet";
import { toAppError } from "@/lib/ipc/client";
import { useReprintVersement } from "@/lib/ipc/credit";
import { useKitchenReprint, usePrebill } from "@/lib/ipc/dining-print";
import { useReprint } from "@/lib/ipc/documents";
import { usePrintFailures, useRetryPrint } from "@/lib/ipc/hardware";
import { useReprintVoucher } from "@/lib/ipc/loyalty";
import type { PrintFailed } from "@/lib/ipc/types";
import { usePrinterStatus } from "@/lib/printer-status";
import { useErrorText } from "@/lib/use-error-text";
import { cn } from "@/lib/utils";

/** The `print-failed` code of a receipt printer installed as a Windows driver only (HW-002). */
export const DRIVER_ONLY = "printer_driver_only";

const toastId = (documentId: string) => `print-failed-${documentId}`;

/** "Réimprimer" for a receipt that did not print (spec §7.1): the original once
 * (`hardware_retry_print`), else a DUPLICATA (`documents_reprint`, or `credit_reprint_receipt`
 * for a versement receipt; either settles the pending original for good). A voucher slip is never
 * pending and has no DUPLICATA: `vouchers_reprint` prints it again. A repair intake slip or a
 * deposit receipt is printed again once from the queue's kept layout (`hardware_retry_print`)
 * and has no DUPLICATA: already reprinted, it only leaves the list. A kitchen ticket goes to the
 * kitchen again (`kitchen_reprint`) and an addition is printed again (`dining_ticket_prebill`).
 * Either way it leaves the list and its toast goes. */
export function useReprintReceipt(): { reprint: (event: PrintFailed) => void; busy: boolean } {
  const errorText = useErrorText();
  const retry = useRetryPrint();
  const duplicate = useReprint();
  const versementDuplicate = useReprintVersement();
  const voucher = useReprintVoucher();
  const kitchen = useKitchenReprint();
  const prebill = usePrebill();
  const done = (event: PrintFailed) => {
    usePrinterStatus.getState().printed(event.documentId);
    toast.dismiss(toastId(event.documentId));
  };
  const reprintDuplicate = (event: PrintFailed) => {
    const handlers = { onSuccess: () => done(event), onError: (e: unknown) => toast.error(errorText(e)) };
    if (event.versement) versementDuplicate.mutate({ entryId: event.documentId, lang: null }, handlers);
    else duplicate.mutate({ id: event.documentId, lang: null }, handlers);
  };
  const reprint = (event: PrintFailed) => {
    const handlers = { onSuccess: () => done(event), onError: (e: unknown) => toast.error(errorText(e)) };
    // M8 restaurant: a kitchen ticket goes to the kitchen again, the addition is printed again
    if (event.slip === "kitchen") return kitchen.mutate(event.documentId, handlers);
    if (event.slip === "pre_bill") return prebill.mutate({ ticketId: event.documentId }, handlers);
    if (event.slip === "voucher") {
      voucher.mutate(event.documentId, {
        onSuccess: () => done(event),
        onError: (error) => toast.error(errorText(error)),
      });
      return;
    }
    retry.mutate(event.documentId, {
      onSuccess: () => done(event),
      onError: (error) => {
        if (toAppError(error).code !== "print_not_pending") toast.error(errorText(error));
        else if (event.slip) done(event);
        else reprintDuplicate(event);
      },
    });
  };
  return {
    reprint,
    busy:
      retry.isPending ||
      duplicate.isPending ||
      versementDuplicate.isPending ||
      voucher.isPending ||
      kitchen.isPending ||
      prebill.isPending,
  };
}

function failedTitleKey(event: PrintFailed): string {
  if (event.slip === "kitchen") return "hardware:print.kitchenFailed";
  if (event.slip === "pre_bill") return "hardware:print.prebillFailed";
  if (event.slip === "voucher") return "hardware:print.voucherFailed";
  if (event.slip === "intake") return "hardware:print.intakeFailed";
  if (event.slip === "deposit") return "hardware:print.depositFailed";
  return event.versement ? "hardware:print.versementFailed" : "hardware:print.failed";
}

/** Shows a receipt or slip that did not print: the printer dot keeps it, and its toast (with
 * Réimprimer) stays until it is reprinted or closed. `description` defaults to the event's code. */
export function useShowPrintFailure(): (event: PrintFailed, description?: string) => void {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const { reprint } = useReprintReceipt();
  return (event, description) => {
    usePrinterStatus.getState().fail(event);
    toast.error(t(failedTitleKey(event), { number: event.number ?? "" }), {
      id: toastId(event.documentId),
      description: description ?? errorText({ code: event.code }),
      duration: Infinity,
      closeButton: true,
      action: { label: t("common:actions.reprint"), onClick: () => reprint(event) },
    });
  };
}

/** Each failed receipt is a toast that stays until it is reprinted or closed, pressable above the
 * sale-done dialog; the printer dot keeps the list. */
export function PrintFailures() {
  const show = useShowPrintFailure();
  usePrintFailures((event) => {
    // HW-002: `DriverPrintFallback` prints that receipt as a page through the Windows driver.
    if (event.code === DRIVER_ONLY && event.printerName) return;
    show(event);
  });
  return null;
}

/** The receipts that did not print, each with Réimprimer (and Ignorer). */
export function UnprintedSheet({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const unprinted = usePrinterStatus((s) => s.unprinted);
  const { reprint, busy } = useReprintReceipt();
  return (
    <Sheet
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("hardware:unprinted.title")}
      description={t("hardware:unprinted.hint")}
    >
      {unprinted.length === 0 ? <p className="text-ink-2">{t("hardware:unprinted.empty")}</p> : null}
      <ul className="flex flex-col gap-2">
        {unprinted.map((event) => (
          <li key={event.documentId} className="flex flex-col gap-2 rounded-card border border-line p-3">
            {event.number === null ? (
              <span className="text-lg font-bold">{t(failedTitleKey(event))}</span>
            ) : (
              <bdi dir="ltr" className="text-lg font-bold">
                {event.number}
              </bdi>
            )}
            <span className="text-ink-2">{errorText({ code: event.code })}</span>
            <div className="flex gap-2">
              <Button variant="primary" disabled={busy} onClick={() => reprint(event)}>
                {t("common:actions.reprint")}
              </Button>
              <Button
                variant="outline"
                onClick={() => {
                  usePrinterStatus.getState().dismiss(event.documentId);
                  toast.dismiss(toastId(event.documentId));
                }}
              >
                {t("hardware:unprinted.dismiss")}
              </Button>
            </div>
          </li>
        ))}
      </ul>
    </Sheet>
  );
}

/** The printer dot: green ready, amber no printer, red after a failed job. With receipts left
 * to reprint it is a button with their count, opening the list. */
export function PrinterDot({ configured, onOpen }: { configured: boolean; onOpen: () => void }) {
  const { t } = useTranslation();
  const failing = usePrinterStatus((s) => s.failing);
  const count = usePrinterStatus((s) => s.unprinted.length);
  const state = failing
    ? { tone: "bg-danger", label: t("hardware:status.failed") }
    : configured
      ? { tone: "bg-ok", label: t("hardware:status.ready") }
      : { tone: "bg-warn", label: t("hardware:status.none") };
  if (count === 0) {
    return (
      <span
        role="img"
        aria-label={state.label}
        title={state.label}
        className="inline-flex items-center gap-1 text-ink-2"
      >
        <Printer aria-hidden className="size-5" />
        <span aria-hidden className={cn("size-2.5 rounded-full", state.tone)} />
      </span>
    );
  }
  const label = t("hardware:unprinted.count", { count });
  return (
    <button
      type="button"
      onClick={onOpen}
      aria-label={label}
      title={`${state.label} · ${label}`}
      className="inline-flex min-h-11 items-center gap-1.5 rounded-full bg-danger-soft px-3 font-semibold text-danger"
    >
      <Printer aria-hidden className="size-5" />
      <span aria-hidden className={cn("size-2.5 rounded-full", state.tone)} />
      <bdi dir="ltr">{count}</bdi>
    </button>
  );
}
