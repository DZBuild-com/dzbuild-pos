import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import type { Lang } from "@/lib/i18n";
import { usePrinterStatus } from "@/lib/printer-status";
import { commands, events } from "./client";
import { qk } from "./keys";
import type {
  HardwareConfig,
  HidSuffix,
  NoSaleInput,
  PrinterConfig,
  PrintFailed,
  ScannerLayout,
} from "./types";

/** The HID scanner as the hardware hub set it (or "Apprendre mon lecteur" found it): the
 * Windows layout it types with, the prefix it sends before each code, and how a code ends
 * (HW-021/022). Every scan field and screen reads codes with these. */
export function useScannerOptions(): { layout: ScannerLayout; prefix: string; suffix: HidSuffix } {
  const scanner = useHardwareConfig().data?.scanner;
  return {
    layout: scanner?.layout ?? "us",
    prefix: scanner?.prefix ?? "",
    suffix: scanner?.suffix ?? "enter_or_tab",
  };
}

/** This PC's printers, receipt options and scanner layout (any signed-in user). */
export function useHardwareConfig(enabled = true) {
  return useQuery({
    queryKey: qk.hardware,
    queryFn: () => commands.hardwareGet(),
    staleTime: 60_000,
    enabled,
  });
}

export function useSaveHardware() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (config: HardwareConfig) => commands.hardwareSave(config),
    onSuccess: (saved) => client.setQueryData(qk.hardware, saved),
  });
}

/** Explicit ("Détecter"): enumerating Windows printers takes a moment. */
export function useDiscoverDevices() {
  return useMutation({ mutationFn: () => commands.hardwareDiscover() });
}

/** The same list for the hardware hub: started as the hub opens, shared by its tabs (the ports of
 * the scale, displays, scanners and drawer); `refetch` is a tab's "Détecter". */
export function useFoundDevices() {
  return useQuery({ queryKey: qk.discovered, queryFn: () => commands.hardwareDiscover() });
}

export function useTestPrint() {
  return useMutation({
    mutationFn: ({ printer, lang }: { printer: PrinterConfig; lang: Lang }) =>
      commands.hardwareTestPrint(printer, lang),
    onSuccess: () => usePrinterStatus.getState().printed(),
  });
}

/** The drawer without a sale (F9), journaled with its reason. */
export function useOpenDrawer() {
  return useMutation({ mutationFn: (input: NoSaleInput) => commands.hardwareOpenDrawer(input) });
}

/** Prints the original of a receipt whose print failed, once. */
export function useRetryPrint() {
  return useMutation({
    mutationFn: (documentId: string) => commands.hardwareRetryPrint(documentId),
    onSuccess: (_, documentId) => usePrinterStatus.getState().printed(documentId),
  });
}

/** Receipts that did not print after a sale (spec §7.1): `onFailed` receives each one. */
export function usePrintFailures(onFailed: (event: PrintFailed) => void): void {
  const handler = useRef(onFailed);
  useEffect(() => {
    handler.current = onFailed;
  }, [onFailed]);
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    void events.printFailed
      .listen((event) => handler.current(event.payload))
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      // A capability without core:event:allow-listen refuses the listen; the sale is safe
      // (printing never blocks it), so the refusal is logged rather than left unhandled.
      .catch((error: unknown) => console.error("print-failed listener refused:", error));
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);
}
