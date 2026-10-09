import { commands, toAppError } from "@/lib/ipc/client";
import { usePrintFailures } from "@/lib/ipc/hardware";
import { useErrorText } from "@/lib/use-error-text";
import { DRIVER_ONLY, useShowPrintFailure } from "./PrintFailures";

/** HW-002: a receipt printer that only takes pages through its Windows driver gets the receipt
 * as a page of its roll, printed silently by the print host: no failure toast for it
 * (`PrintFailures` skips the code). A page print that fails too is shown like any receipt that did
 * not print (toast, printer dot, Réimprimer), unless another window already printed it
 * (`print_not_pending`). Mounted once per window, beside `PrintFailures`. */
export function DriverPrintFallback() {
  const errorText = useErrorText();
  const show = useShowPrintFailure();
  usePrintFailures((event) => {
    if (event.code !== DRIVER_ONLY || !event.printerName) return;
    commands
      .documentsPrintReceiptRoll(event.documentId, event.printerName, event.paperWidthMm ?? 80)
      .catch((error: unknown) => {
        const code = toAppError(error).code;
        if (code === "print_not_pending") return;
        show({ ...event, code }, errorText(error));
      });
  });
  return null;
}
