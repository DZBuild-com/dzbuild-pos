import { QueryClientProvider, type QueryClient } from "@tanstack/react-query";
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { useDocuments, useReprint } from "@/lib/ipc/documents";
import { usePrintFailures, useSaveHardware } from "@/lib/ipc/hardware";
import { createQueryClient } from "@/lib/query";
import { mockCommands } from "@/test/render";

function wrapper(client: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

describe("printing hooks", () => {
  it("hands every print-failed event to the listener", async () => {
    mockCommands({});
    const onFailed = vi.fn();
    renderHook(() => usePrintFailures(onFailed), { wrapper: wrapper(createQueryClient()) });
    await waitFor(async () => {
      await emit("print-failed", { documentId: "d-1", number: "TK2026-C1-000123", code: "printer_offline" });
      expect(onFailed).toHaveBeenCalledWith({
        documentId: "d-1",
        number: "TK2026-C1-000123",
        code: "printer_offline",
      });
    });
  });

  it("logs a refused listen instead of leaving it unhandled", async () => {
    const refusal =
      "event.listen not allowed. Permissions associated with this command: core:event:allow-listen";
    mockIPC((cmd) => (cmd === "plugin:event|listen" ? Promise.reject(refusal) : null));
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      renderHook(() => usePrintFailures(vi.fn()), { wrapper: wrapper(createQueryClient()) });
      await waitFor(() => expect(logged).toHaveBeenCalledWith(expect.any(String), refusal));
    } finally {
      logged.mockRestore();
    }
  });

  it("asks for a duplicate in the chosen language", async () => {
    const received: unknown[] = [];
    mockCommands({
      documents_reprint: (args) => {
        received.push(args);
        return { printer: "Caisse 1", bytes: 1_200, raster: false };
      },
    });
    const { result } = renderHook(() => useReprint(), { wrapper: wrapper(createQueryClient()) });
    await result.current.mutateAsync({ id: "d-1", lang: "ar" });
    expect(received).toEqual([{ id: "d-1", lang: "ar" }]);
  });

  it("pages the history by validation time", async () => {
    const received: unknown[] = [];
    const page = Array.from({ length: 100 }, (_, i) => ({
      id: `d-${i}`,
      docType: "ticket",
      number: `TK2026-C1-${String(1000 - i).padStart(6, "0")}`,
      status: "validated",
      businessDate: "2026-09-28",
      validatedAt: 1_790_593_200_000 - i,
      totalDue: 15_000,
      userId: "u-cashier",
      terminalId: "t-1",
      parentId: null,
    }));
    mockCommands({
      documents_list: (args) => {
        received.push(args);
        return received.length === 1 ? page : [];
      },
    });
    const client = createQueryClient();
    const { result } = renderHook(
      () =>
        useDocuments({
          docType: null,
          terminalId: null,
          cashSessionId: null,
          businessDate: null,
          numberPrefix: null,
        }),
      { wrapper: wrapper(client) },
    );
    await waitFor(() => expect(result.current.data?.pages[0]).toHaveLength(100));
    expect(result.current.hasNextPage).toBe(true);
    await result.current.fetchNextPage();
    expect(received[1]).toMatchObject({
      query: { beforeValidatedAt: 1_790_593_200_000 - 99, beforeId: "d-99", limit: 100 },
    });
  });

  it("keeps the saved hardware settings in the cache", async () => {
    const config = {
      version: 1,
      printers: [],
      receiptPrinterId: null,
      receipt: {
        printByDefault: false,
        openDrawerOnCash: true,
        drawerBeforePrint: false,
        logo: true,
        secondLang: null,
      },
      scanner: { layout: "azerty" },
      documents: { defaultPrinter: null, kinds: [], marginsMm: 12 },
    };
    mockCommands({ hardware_save: ({ config: sent }) => sent });
    const client = createQueryClient();
    const { result } = renderHook(() => useSaveHardware(), { wrapper: wrapper(client) });
    await result.current.mutateAsync(config as never);
    expect(client.getQueryData(["hardware"])).toEqual(config);
  });
});
