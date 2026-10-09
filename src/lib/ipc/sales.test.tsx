import { QueryClientProvider, type QueryClient } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";
import { useCloseSession, useCurrentSession, useOpenSession } from "@/lib/ipc/cash";
import { useReturnQuote } from "@/lib/ipc/returns";
import { useCreateSale, useSalePreview } from "@/lib/ipc/sales";
import { createQueryClient } from "@/lib/query";
import { cashSessionFixture, previewFixture, saleDoneFixture } from "@/test/fixtures-m1";
import { mockCommands } from "@/test/render";

function wrapper(client: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

const cart = {
  lines: [
    {
      productId: "p-milk",
      description: null,
      qty: 2_000,
      unitPrice: null,
      discount: { kind: "none" as const },
      taxRateId: null,
      note: null,
      expectedUnitPrice: 15_000,
    },
  ],
  priceLevelId: null,
  customerId: null,
  lang: "fr" as const,
  note: null,
};

describe("till hooks", () => {
  it("previews the cart with the tenders and never for an empty cart", async () => {
    const received: unknown[] = [];
    mockCommands({
      sales_preview: (args) => {
        received.push(args);
        return previewFixture(30_000, 30_000);
      },
    });
    const client = createQueryClient();
    const empty = renderHook(() => useSalePreview(null), { wrapper: wrapper(client) });
    expect(empty.result.current.fetchStatus).toBe("idle");
    const input = {
      cart,
      tenders: [
        { method: "cash" as const, amount: 50_000, reference: null, chequeBank: null, chequeDate: null },
      ],
      storeCredit: null,
    };
    const { result } = renderHook(() => useSalePreview(input), { wrapper: wrapper(client) });
    // the backend's figure as is: 300 DA and the fixture's 5 DA of duty
    await waitFor(() => expect(result.current.data?.settlement.totalDue).toBe(30_500));
    expect(received).toEqual([{ input }]);
  });

  it("shows no figure once the cart is emptied, not the last ticket's", async () => {
    mockCommands({ sales_preview: () => previewFixture(30_000, 30_000) });
    const client = createQueryClient();
    const input = { cart, tenders: [], storeCredit: null };
    const { result, rerender } = renderHook(
      ({ current }: { current: typeof input | null }) => useSalePreview(current),
      { wrapper: wrapper(client), initialProps: { current: input as typeof input | null } },
    );
    await waitFor(() => expect(result.current.data?.settlement.totalDue).toBe(30_500));
    // the ticket is committed (or cancelled) and the cart cleared: nothing is due any more
    rerender({ current: null });
    expect(result.current.data).toBeUndefined();
    expect(result.current.fetchStatus).toBe("idle");
  });

  it("refreshes the session, the held list and the history after a sale", async () => {
    mockCommands({ sales_create: () => saleDoneFixture });
    const client = createQueryClient();
    client.setQueryData(["cash", "current"], cashSessionFixture);
    client.setQueryData(["documents", "list", {}], { pages: [], pageParams: [] });
    const { result } = renderHook(() => useCreateSale(), { wrapper: wrapper(client) });
    await result.current.mutateAsync({ sale: { cart, payments: [] }, printReceipt: true });
    expect(client.getQueryState(["cash", "current"])?.isInvalidated).toBe(true);
    expect(client.getQueryState(["documents", "list", {}])?.isInvalidated).toBe(true);
  });

  it("stores the opened session and forgets it at the close", async () => {
    const calls = mockCommands({
      cash_current: () => null,
      cash_open_session: ({ openingFloat }) => ({ ...cashSessionFixture, openingFloat }),
      cash_close_session: () => ({ z: { zNumber: "ZC1-000031" }, backup: null, backupError: null }),
    });
    const client = createQueryClient();
    const current = renderHook(() => useCurrentSession(), { wrapper: wrapper(client) });
    await waitFor(() => expect(current.result.current.data).toBeNull());
    const open = renderHook(() => useOpenSession(), { wrapper: wrapper(client) });
    await open.result.current.mutateAsync(1_000_000);
    await waitFor(() => expect(current.result.current.data?.openingFloat).toBe(1_000_000));
    const close = renderHook(() => useCloseSession(), { wrapper: wrapper(client) });
    await close.result.current.mutateAsync({ counts: [], notes: null });
    // the mounted session query is refetched at once: the till reads closed again
    await waitFor(() => expect(current.result.current.data).toBeNull());
    expect(calls.filter((c) => c === "cash_current")).toHaveLength(2);
  });

  it("quotes a return with the typed input", async () => {
    const received: unknown[] = [];
    mockCommands({
      returns_quote: (args) => {
        received.push(args);
        return { value: 15_000, lines: [] };
      },
    });
    const { result } = renderHook(() => useReturnQuote(), { wrapper: wrapper(createQueryClient()) });
    const input = {
      originalId: "d-1",
      lines: [
        {
          originLineId: "l-1",
          productId: null,
          qty: 1_000,
          unitPrice: null,
          restock: true,
          reason: "changed_mind" as const,
          note: null,
        },
      ],
      refundMethod: "cash" as const,
      refundReference: null,
      lang: "fr" as const,
      note: null,
    };
    await expect(result.current.mutateAsync(input)).resolves.toEqual({ value: 15_000, lines: [] });
    // M8: no pack extras unless the till passes them (the serials coming back, the voucher).
    expect(received).toEqual([{ input, extras: null }]);
  });
});
