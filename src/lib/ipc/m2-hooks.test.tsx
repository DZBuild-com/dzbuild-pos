import { emit } from "@tauri-apps/api/event";
import { renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useCollect, useCreditQuote } from "@/lib/ipc/credit";
import { useCustomerSearch } from "@/lib/ipc/customers";
import { useRecordExpense } from "@/lib/ipc/expenses";
import { useExport } from "@/lib/ipc/exports";
import { useApplyImport, useImportPreview, useImportProgress } from "@/lib/ipc/imports";
import { useReport } from "@/lib/ipc/reports";
import { useCountLines, useScanCount } from "@/lib/ipc/stock-m2";
import { createQueryClient } from "@/lib/query";
import {
  collectQuoteFixture,
  countFixture,
  countLinesFixture,
  customerFixture,
  customerHitsFixture,
  expenseFixture,
  importFileFixture,
  importPreviewFixture,
  reportViewFixture,
} from "@/test/fixtures-m2";
import { mockCommands, providers } from "@/test/render";

const versement = {
  customerId: customerFixture.id,
  amount: 200_000,
  method: "cash" as const,
  reference: null,
  chequeBank: null,
  chequeDate: null,
  allocateTo: [],
  note: null,
};

describe("M2 hooks", () => {
  it("searches customers and quotes a versement from the backend", async () => {
    const calls: string[] = [];
    mockCommands({
      customers_search: () => customerHitsFixture,
      credit_quote: (args) => {
        const { input } = args as { input: { amount: number } };
        calls.push(`quote:${input.amount}`);
        return collectQuoteFixture;
      },
    });
    const search = renderHook(() => useCustomerSearch({ text: "kar", limit: 20 }), { wrapper: providers() });
    await waitFor(() => expect(search.result.current.data?.[0]?.name).toBe("Karim Benali"));
    const quote = renderHook(() => useCreditQuote(versement), { wrapper: providers() });
    await waitFor(() => expect(quote.result.current.data?.stampDuty).toBe(2_000));
    expect(calls).toEqual(["quote:200000"]);
  });

  it("never quotes an empty versement", () => {
    const calls = mockCommands({ credit_quote: () => collectQuoteFixture });
    const idle = renderHook(() => useCreditQuote({ ...versement, amount: 0 }), { wrapper: providers() });
    expect(idle.result.current.fetchStatus).toBe("idle");
    expect(calls).toEqual([]);
  });

  it("invalidates the customer, the kridi and the cash after a versement", async () => {
    mockCommands({
      credit_collect: () => ({
        collected: { entry: {}, quote: collectQuoteFixture, customer: customerFixture, allocations: [] },
        printQueued: true,
      }),
    });
    const client = createQueryClient();
    const keys = [
      ["customers", customerFixture.id],
      ["credit", customerFixture.id],
      ["cash", "current"],
    ];
    for (const queryKey of keys) client.setQueryData(queryKey, {});
    const { result } = renderHook(() => useCollect(), { wrapper: providers(client) });
    const done = await result.current.mutateAsync({ input: versement, printReceipt: true });
    expect(done.printQueued).toBe(true);
    for (const queryKey of keys) expect(client.getQueryState(queryKey)?.isInvalidated).toBe(true);
  });

  it("scans a code into a count and fetches the count's lines again", async () => {
    const scans: unknown[] = [];
    const calls = mockCommands({
      counts_lines: () => countLinesFixture,
      counts_scan: (args) => {
        scans.push(args);
        return { ...countLinesFixture[0], countedQty: 1_000 };
      },
    });
    const client = createQueryClient();
    const lines = renderHook(() => useCountLines(countFixture.id), { wrapper: providers(client) });
    await waitFor(() => expect(lines.result.current.data).toHaveLength(2));
    const scan = renderHook(() => useScanCount(), { wrapper: providers(client) });
    const line = await scan.result.current.mutateAsync({ id: countFixture.id, code: "6130000000015" });
    expect(line.countedQty).toBe(1_000);
    expect(scans).toEqual([{ id: countFixture.id, code: "6130000000015" }]);
    expect(calls.filter((c) => c === "counts_lines")).toHaveLength(2);
  });

  it("records an expense from the drawer and refreshes the cash session", async () => {
    const sent: unknown[] = [];
    mockCommands({
      expenses_record: (args) => {
        sent.push(args.input);
        return expenseFixture;
      },
    });
    const client = createQueryClient();
    client.setQueryData(["cash", "current"], {});
    const { result } = renderHook(() => useRecordExpense(), { wrapper: providers(client) });
    const input = {
      categoryId: expenseFixture.categoryId,
      amount: 80_000,
      method: "cash" as const,
      fromTill: true,
      reference: null,
      supplierId: null,
      businessDate: null,
      note: null,
    };
    expect((await result.current.mutateAsync(input)).fromTill).toBe(true);
    expect(sent).toEqual([input]);
    expect(client.getQueryState(["cash", "current"])?.isInvalidated).toBe(true);
  });

  it("previews the opened file, applies the checked options and refreshes the products", async () => {
    const applied: unknown[] = [];
    mockCommands({
      imports_preview: () => importPreviewFixture,
      imports_apply: (args) => {
        applied.push(args);
        return { id: "j1", kind: "products", status: "done", created: 3, undoable: true };
      },
    });
    const client = createQueryClient();
    const idle = renderHook(() => useImportPreview(null, 0, "products"), { wrapper: providers(client) });
    expect(idle.result.current.fetchStatus).toBe("idle");
    const preview = renderHook(() => useImportPreview(importFileFixture.token, 0, "products"), {
      wrapper: providers(client),
    });
    await waitFor(() =>
      expect(preview.result.current.data?.suggested).toEqual(importPreviewFixture.suggested),
    );
    for (const queryKey of [
      ["products", "grid"],
      ["imports", "jobs"],
    ])
      client.setQueryData(queryKey, []);
    const { result } = renderHook(() => useApplyImport(), { wrapper: providers(client) });
    const options = {
      entity: "products" as const,
      policy: "create_only" as const,
      fileName: importFileFixture.fileName,
      headerRow: 0,
      mapping: importPreviewFixture.suggested,
    };
    await result.current.mutateAsync({ token: importFileFixture.token, sheet: 0, options });
    expect(applied).toEqual([{ token: importFileFixture.token, sheet: 0, options }]);
    expect(client.getQueryState(["products", "grid"])?.isInvalidated).toBe(true);
    expect(client.getQueryState(["imports", "jobs"])?.isInvalidated).toBe(true);
  });

  it("hands every import-progress event to the listener", async () => {
    mockCommands({});
    const onProgress = vi.fn();
    renderHook(() => useImportProgress(onProgress), { wrapper: providers() });
    const progress = {
      jobId: "j1",
      done: 500,
      total: 1_200,
      created: 480,
      updated: 0,
      skipped: 5,
      failed: 15,
    };
    await waitFor(async () => {
      await emit("import-progress", progress);
      expect(onProgress).toHaveBeenCalledWith(progress);
    });
  });

  it("waits for a report's required filter and exports the request as given", async () => {
    const exported: unknown[] = [];
    const calls = mockCommands({
      reports_run: () => reportViewFixture,
      exports_save: (args) => {
        exported.push(args.request);
        return { path: "C:/Exports/sales-by-product-2026-09-29.xlsx", rows: 2 };
      },
    });
    const waiting = renderHook(() => useReport(null), { wrapper: providers() });
    expect(waiting.result.current.fetchStatus).toBe("idle");
    const request = { kind: "sales_by_product" as const, from: "2026-09-01", to: "2026-09-30" };
    const report = renderHook(() => useReport(request), { wrapper: providers() });
    await waitFor(() => expect(report.result.current.data?.labels[4]).toBe("Net TTC"));
    const { result } = renderHook(() => useExport(), { wrapper: providers() });
    const exportRequest = {
      target: { kind: "report" as const, request },
      format: "xlsx" as const,
      lang: "fr" as const,
    };
    expect((await result.current.mutateAsync(exportRequest))?.rows).toBe(2);
    expect(exported).toEqual([exportRequest]);
    expect(calls.filter((c) => c === "reports_run")).toHaveLength(1);
  });
});
