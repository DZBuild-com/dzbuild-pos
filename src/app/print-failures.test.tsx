import { emit } from "@tauri-apps/api/event";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Toaster } from "sonner";
import { expect, it } from "vitest";
import { usePrinterStatus } from "@/lib/printer-status";
import { mockCommands, renderWithProviders } from "@/test/render";
import { PrintFailures } from "./PrintFailures";

// Task 21 review: a voucher slip is never pending a retry and is no document, so its Réimprimer
// prints the slip again (`vouchers_reprint`) instead of the receipt's retry or DUPLICATA.
it("reprints a voucher slip that did not print with vouchers_reprint", async () => {
  const reprinted: unknown[] = [];
  const calls = mockCommands({
    vouchers_reprint: (args) => {
      reprinted.push(args);
      return { voucher: { code: "BA2026-000001" }, printQueued: true };
    },
  });
  const user = userEvent.setup();
  renderWithProviders(
    <>
      <Toaster />
      <PrintFailures />
    </>,
  );
  await emit("print-failed", {
    documentId: "BA2026-000001",
    number: "BA2026-000001",
    code: "printer_offline",
    versement: false,
    slip: "voucher",
  });
  expect(await screen.findByText("Le bon BA2026-000001 n'a pas été imprimé")).toBeInTheDocument();
  expect(usePrinterStatus.getState().unprinted).toHaveLength(1);
  await user.click(screen.getByRole("button", { name: "Réimprimer" }));
  await waitFor(() => expect(reprinted).toEqual([{ code: "BA2026-000001" }]));
  expect(calls).not.toContain("hardware_retry_print");
  expect(calls).not.toContain("documents_reprint");
  await waitFor(() => expect(usePrinterStatus.getState().unprinted).toEqual([]));
});

// A repair intake slip or a deposit receipt (pack slips) is named by its own
// number and printed again once from the queue's kept layout (`hardware_retry_print`); it has
// no DUPLICATA, so a slip already reprinted only leaves the list.
it("names a deposit receipt that did not print and retries its slip, never a DUPLICATA", async () => {
  const retried: unknown[] = [];
  const calls = mockCommands({
    hardware_retry_print: (args) => {
      retried.push(args);
      return null;
    },
  });
  const user = userEvent.setup();
  renderWithProviders(
    <>
      <Toaster />
      <PrintFailures />
    </>,
  );
  await emit("print-failed", {
    documentId: "AC2026-000004",
    number: "AC2026-000004",
    code: "printer_offline",
    versement: false,
    slip: "deposit",
  });
  expect(await screen.findByText("Le reçu d'arrhes AC2026-000004 n'a pas été imprimé")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Réimprimer" }));
  await waitFor(() => expect(retried).toEqual([{ documentId: "AC2026-000004" }]));
  expect(calls).not.toContain("documents_reprint");
  await waitFor(() => expect(usePrinterStatus.getState().unprinted).toEqual([]));
});
