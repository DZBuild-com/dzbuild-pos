import { create } from "zustand";
import type { PrintFailed } from "@/lib/ipc/types";

/** The receipts that did not print (newest first) and whether the printer's last job failed: the
 * printer dot turns red while it does, and lists the receipts to reprint (spec §7.1). */
export const usePrinterStatus = create<{
  failing: boolean;
  unprinted: PrintFailed[];
  /** A receipt did not print. */
  fail: (event: PrintFailed) => void;
  /** Something printed: the printer works again; `documentId`, when given, is printed now. */
  printed: (documentId?: string) => void;
  /** The cashier drops a receipt from the list without printing it. */
  dismiss: (documentId: string) => void;
  clear: () => void;
}>()((set) => ({
  failing: false,
  unprinted: [],
  fail: (event) =>
    set((s) => ({
      failing: true,
      unprinted: [event, ...s.unprinted.filter((u) => u.documentId !== event.documentId)],
    })),
  printed: (documentId) =>
    set((s) => ({
      failing: false,
      unprinted: documentId ? s.unprinted.filter((u) => u.documentId !== documentId) : s.unprinted,
    })),
  dismiss: (documentId) =>
    set((s) => ({ unprinted: s.unprinted.filter((u) => u.documentId !== documentId) })),
  clear: () => set({ failing: false, unprinted: [] }),
}));
