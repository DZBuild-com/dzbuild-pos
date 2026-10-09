import { act, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { ScanInput } from "@/components/scan-input";
import { useSerialScans } from "@/lib/ipc/devices";
import { setBurstGuard } from "@/lib/scanner";
import { mockCommands, renderWithProviders } from "@/test/render";
import { scannerKeys } from "@/test/scan";

/** `]E0`, then the digits: an AIM identifier the scanner sends before each code. */
const prefixed = (code: string, enter = true) => [
  { code: "BracketRight", key: "]" },
  { code: "KeyE", key: "E", shiftKey: true },
  { code: "Digit0", key: "0" },
  ...[...code].map((d) => ({ code: `Digit${d}`, key: d })),
  ...(enter ? [{ code: "Enter", key: "Enter" }] : []),
];

/** The hub's scanner settings; resolves once the page has read them. */
async function hub(scanner: Record<string, unknown>) {
  let served = false;
  mockCommands({
    hardware_get: () => {
      served = true;
      return { version: 2, printers: [], receiptPrinterId: null, scanner };
    },
  });
  return async () => {
    await waitFor(() => expect(served).toBe(true));
    // the settings reach the field
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
  };
}

/** A screen listening to the serial scanner, with a dialog's own scan field. */
function Screen({
  onScreen,
  onField,
}: {
  onScreen: (code: string) => void;
  onField: (code: string) => void;
}) {
  useSerialScans(onScreen);
  const [value, setValue] = useState("");
  return (
    <ScanInput aria-label="Code" value={value} onChange={(e) => setValue(e.target.value)} onScan={onField} />
  );
}

beforeEach(() => setBurstGuard(true));
afterEach(() => setBurstGuard(false));

describe("scan field", () => {
  it("removes the prefix the hub set for the scanner (HW-022)", async () => {
    const loaded = await hub({ layout: "us", prefix: "]E0" });
    const scanned: string[] = [];
    renderWithProviders(<ScanInput aria-label="Code" onScan={(code) => scanned.push(code)} />);
    await loaded();
    scannerKeys(screen.getByLabelText("Code"), prefixed("6130000000015"));
    expect(scanned).toEqual(["6130000000015"]);
  });

  it("ends a scan on a silence when the scanner sends no Enter", async () => {
    const loaded = await hub({ layout: "us", prefix: "", suffix: "silence" });
    const scanned: string[] = [];
    renderWithProviders(<ScanInput aria-label="Code" onScan={(code) => scanned.push(code)} />);
    await loaded();
    scannerKeys(screen.getByLabelText("Code"), prefixed("6130000000015", false).slice(3));
    expect(scanned).toEqual([]);
    await waitFor(() => expect(scanned).toEqual(["6130000000015"]));
  });

  it("takes a serial scanner's codes while it has the focus, the screen otherwise (HW-020)", async () => {
    mockCommands({});
    const screenCodes: string[] = [];
    const fieldCodes: string[] = [];
    renderWithProviders(<Screen onScreen={(c) => screenCodes.push(c)} onField={(c) => fieldCodes.push(c)} />);
    // each code is sent again until its listener is in place (they subscribe asynchronously)
    await waitFor(async () => {
      if (screenCodes.length === 0) await act(() => emit("serial-scanned", { code: "6130000000015" }));
      expect(screenCodes).toEqual(["6130000000015"]);
    });
    act(() => screen.getByLabelText("Code").focus());
    await waitFor(async () => {
      if (fieldCodes.length === 0) await act(() => emit("serial-scanned", { code: "6130000000022" }));
      expect(fieldCodes).toEqual(["6130000000022"]);
    });
    expect(screenCodes).toEqual(["6130000000015"]);
  });
});
