import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import type { Lang } from "@/lib/i18n";
import { commands, events } from "./client";
import { qk } from "./keys";
import type {
  DeviceStatus,
  DeviceStatusChanged,
  DisplayFrame,
  LabelPrinterConfig,
  SerialScanned,
  VfdConfig,
  WeightChanged,
  WeightEvent,
} from "./types";

type Listen<T> = (cb: (e: { payload: T }) => void) => Promise<() => void>;

// Module-level: `events.x.listen` is a new function at each access, so hooks keep these instead.
const listenWeight: Listen<WeightChanged> = (cb) => events.weightChanged.listen(cb);
const listenStatus: Listen<DeviceStatusChanged> = (cb) => events.deviceStatusChanged.listen(cb);
const listenSerial: Listen<SerialScanned> = (cb) => events.serialScanned.listen(cb);

/** Listens to a tauri-specta event for the component's lifetime. */
function useEvent<T>(listen: Listen<T>, onEvent: (payload: T) => void, enabled = true) {
  const handler = useRef(onEvent);
  useEffect(() => {
    handler.current = onEvent;
  }, [onEvent]);
  useEffect(() => {
    if (!enabled) return undefined;
    let stop: (() => void) | undefined;
    let cancelled = false;
    listen((e) => handler.current(e.payload))
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      .catch((error: unknown) => console.warn("device event", error));
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [listen, enabled]);
}

/** Each device's state (off, connected, disconnected with its error code), kept live. */
export function useDeviceStatus() {
  const client = useQueryClient();
  const query = useQuery({ queryKey: qk.devices, queryFn: () => commands.devicesStatus() });
  useEvent(listenStatus, (changed) => {
    client.setQueryData<DeviceStatus[]>(qk.devices, (old = []) => [
      ...old.filter((d) => d.device !== changed.device),
      { device: changed.device, state: changed.state },
    ]);
  });
  return query;
}

/** The scale's live reading (VRT-010): the latest one, then every change. */
export function useWeight(enabled: boolean): WeightEvent | null {
  const [reading, setReading] = useState<WeightEvent | null>(null);
  useEffect(() => {
    if (!enabled) return;
    commands
      .scaleLatest()
      // an event that came first is newer than this answer
      .then((latest) => setReading((current) => current ?? latest))
      .catch((error: unknown) => console.warn("scale", error));
  }, [enabled]);
  useEvent(listenWeight, (changed) => setReading(changed.reading), enabled);
  return reading;
}

export function useTare() {
  return useMutation({ mutationFn: () => commands.scaleTare() });
}

export function useClearTare() {
  return useMutation({ mutationFn: () => commands.scaleClearTare() });
}

export function useZero() {
  return useMutation({ mutationFn: () => commands.scaleZero() });
}

/** The next stable weight within `timeoutMs` (100 to 3 000 ms). */
export function useReadStable() {
  return useMutation({ mutationFn: (timeoutMs: number) => commands.scaleReadStable(timeoutMs) });
}

/** The settings' "Detect" on a serial port. */
export function useScaleProbe() {
  return useMutation({ mutationFn: (port: string) => commands.scaleProbe(port) });
}

/** Fire-and-forget: a display problem never interrupts a sale (it shows as a device state). */
export function useShowOnDisplay(): (frame: DisplayFrame) => void {
  return useCallback((frame: DisplayFrame) => {
    commands.displayShow(frame).catch((error: unknown) => console.warn("customer display", error));
  }, []);
}

export function useMonitors(enabled: boolean) {
  return useQuery({ queryKey: ["devices", "monitors"], queryFn: () => commands.displayMonitors(), enabled });
}

export function useOpenDisplayPreview() {
  return useMutation({ mutationFn: () => commands.displayOpenPreview() });
}

export function useCloseDisplay() {
  return useMutation({ mutationFn: () => commands.displayClose() });
}

export function useTestVfd() {
  return useMutation({ mutationFn: (config: VfdConfig) => commands.displayTestVfd(config) });
}

/** HW-008: the printer's paper, cover and reachability, every 30 s. */
export function usePrinterStatus(printerId: string | null) {
  return useQuery({
    queryKey: ["devices", "printer", printerId],
    queryFn: () => commands.hardwarePrinterStatus(printerId as string),
    enabled: printerId !== null,
    refetchInterval: 30_000,
  });
}

/** HW-009: saved printers Windows lost, changed the driver of, or holds offline. */
export function useDoctor() {
  return useMutation({ mutationFn: () => commands.hardwareDoctor() });
}

export function useTestLabel() {
  return useMutation({
    mutationFn: ({ printer, lang }: { printer: LabelPrinterConfig; lang: Lang }) =>
      commands.hardwareTestLabel(printer, lang),
  });
}

/** Scan fields that have the focus now: the serial scanner's codes are theirs. */
let fieldsReading = 0;

/** Codes from a serial (USB-COM) scanner, like keyboard scans (HW-020), for a whole screen: a
 * code goes to the scan field that has the focus instead (a dialog's own field). */
export function useSerialScans(onScan: (code: string) => void, enabled = true): void {
  useEvent(
    listenSerial,
    (e) => {
      if (fieldsReading === 0) onScan(e.code);
    },
    enabled,
  );
}

/** The serial scanner's codes for one scan field while it has the focus (`focused`): the
 * screen's own listener ([`useSerialScans`]) leaves them to it. */
export function useFieldSerialScans(onScan: (code: string) => void, focused: boolean): void {
  useEffect(() => {
    if (!focused) return undefined;
    fieldsReading += 1;
    return () => {
      fieldsReading -= 1;
    };
  }, [focused]);
  useEvent(listenSerial, (e) => onScan(e.code), focused);
}
