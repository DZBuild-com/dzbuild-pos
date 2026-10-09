import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { screen, waitFor } from "@testing-library/react";
import { Toaster } from "sonner";
import { expect, it } from "vitest";
import { usePrinterStatus } from "@/lib/printer-status";
import { mockCommands, renderWithProviders } from "@/test/render";
import { DriverPrintFallback } from "./DriverPrintFallback";

// `src/test/setup.ts` unmounts, then clears the IPC mocks, after each test.
function recordCalls() {
  const calls: Array<{ cmd: string; args: unknown }> = [];
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args });
      return null;
    },
    { shouldMockEvents: true },
  );
  return calls;
}

it("prints a receipt through the Windows driver when the printer only takes pages", async () => {
  const calls = recordCalls();
  renderWithProviders(<DriverPrintFallback />);
  await emit("print-failed", {
    documentId: "d1",
    number: "TK2026-C1-000001",
    code: "printer_driver_only",
    versement: false,
    printerName: "HP LaserJet",
    paperWidthMm: 80,
  });
  await waitFor(() => expect(calls.some((c) => c.cmd === "documents_print_receipt_roll")).toBe(true));
  expect(calls.find((c) => c.cmd === "documents_print_receipt_roll")?.args).toMatchObject({
    documentId: "d1",
    printerName: "HP LaserJet",
    widthMm: 80,
  });
});

it("leaves the other print failures to their toast", async () => {
  const calls = recordCalls();
  renderWithProviders(<DriverPrintFallback />);
  await emit("print-failed", {
    documentId: "d2",
    number: "TK2026-C1-000002",
    code: "printer_offline",
    versement: false,
    printerName: null,
    paperWidthMm: null,
  });
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(calls.some((c) => c.cmd === "documents_print_receipt_roll")).toBe(false);
});

const driverOnly = {
  documentId: "d3",
  number: "TK2026-C1-000003",
  code: "printer_driver_only",
  versement: false,
  printerName: "HP LaserJet",
  paperWidthMm: 80,
};

// Task 21 review: a page print that fails is a failed receipt like any other, never only a log.
it("shows a receipt whose page print failed, with the printer dot's list", async () => {
  mockCommands({ documents_print_receipt_roll: () => Promise.reject({ code: "print_host_unavailable" }) });
  renderWithProviders(
    <>
      <Toaster />
      <DriverPrintFallback />
    </>,
  );
  await emit("print-failed", driverOnly);
  expect(await screen.findByText("Le ticket TK2026-C1-000003 n'a pas été imprimé")).toBeInTheDocument();
  expect(screen.getByText(/L'impression A4 n'a pas pu démarrer/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Réimprimer" })).toBeInTheDocument();
  expect(usePrinterStatus.getState()).toMatchObject({
    failing: true,
    unprinted: [{ documentId: "d3", code: "print_host_unavailable" }],
  });
});

it("stays quiet when another window already printed the receipt", async () => {
  const calls = mockCommands({
    documents_print_receipt_roll: () => Promise.reject({ code: "print_not_pending" }),
  });
  renderWithProviders(
    <>
      <Toaster />
      <DriverPrintFallback />
    </>,
  );
  await emit("print-failed", driverOnly);
  await waitFor(() => expect(calls).toContain("documents_print_receipt_roll"));
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(screen.queryByText("Le ticket TK2026-C1-000003 n'a pas été imprimé")).toBeNull();
  expect(usePrinterStatus.getState().unprinted).toEqual([]);
});
