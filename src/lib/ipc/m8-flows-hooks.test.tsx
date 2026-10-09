import { renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useCollectInstalment } from "@/lib/ipc/instalments";
import { useCloseRound } from "@/lib/ipc/wholesale";
import { usePickup, useVoucherDescribe } from "@/lib/ipc/services";
import { createQueryClient } from "@/lib/query";
import { mockCommands, providers } from "@/test/render";

/** Queries a screen would have read before the write, under the M2 keys a collection touches. */
const M2_KEYS = [
  ["customers", "c1"],
  ["credit", "c1"],
  ["cash", "current"],
];

describe("M8 flow hooks", () => {
  it("refreshes the customer, his kridi and the drawer after a pickup", async () => {
    const sent: unknown[] = [];
    mockCommands({
      service_pickup: (args) => {
        sent.push(args);
        return { picked: { ticket: {}, sale: {} }, printQueued: true };
      },
    });
    const client = createQueryClient();
    for (const queryKey of [...M2_KEYS, ["services", "one", "t1"]]) client.setQueryData(queryKey, {});
    const { result } = renderHook(() => usePickup(), { wrapper: providers(client) });
    const input = { id: "t1", payments: [], extras: {} };
    await result.current.mutateAsync({ input, printReceipt: true });
    expect(sent).toEqual([{ input, printReceipt: true }]);
    for (const queryKey of [...M2_KEYS, ["services", "one", "t1"]]) {
      expect(client.getQueryState(queryKey)?.isInvalidated).toBe(true);
    }
  });

  it("refreshes the plans and the M2 accounts after a collection or a round's close", async () => {
    mockCommands({
      instalments_collect: () => ({ collected: {}, printQueued: true }),
      rounds_close: () => ({ round: { round: {}, versements: [] }, printQueued: true }),
    });
    const client = createQueryClient();
    const keys = [...M2_KEYS, ["instalments", "one", "p1"], ["rounds", "one", "r1"]];
    for (const queryKey of keys) client.setQueryData(queryKey, {});
    const collect = renderHook(() => useCollectInstalment(), { wrapper: providers(client) });
    const payment = {
      amount: 1_500_000,
      method: "cash" as const,
      reference: null,
      chequeBank: null,
      chequeDate: null,
      note: null,
    };
    await collect.result.current.mutateAsync({ planId: "p1", payment, printReceipt: true });
    for (const queryKey of [...M2_KEYS, ["instalments", "one", "p1"]]) {
      expect(client.getQueryState(queryKey)?.isInvalidated).toBe(true);
    }
    for (const queryKey of M2_KEYS) client.setQueryData(queryKey, {});
    const close = renderHook(() => useCloseRound(), { wrapper: providers(client) });
    await close.result.current.mutateAsync("r1");
    for (const queryKey of [...M2_KEYS, ["rounds", "one", "r1"]]) {
      expect(client.getQueryState(queryKey)?.isInvalidated).toBe(true);
    }
  });

  it("describes a voucher reference only once one is typed", async () => {
    const calls = mockCommands({
      vouchers_describe: () => ({
        target: { kind: "deposit", ownerType: "service_ticket", number: "RP2026-000001" },
        balance: 200_000,
        customerId: "c1",
      }),
    });
    const idle = renderHook(() => useVoucherDescribe("  "), { wrapper: providers() });
    expect(idle.result.current.fetchStatus).toBe("idle");
    const typed = renderHook(() => useVoucherDescribe("RP2026-000001"), { wrapper: providers() });
    await waitFor(() => expect(typed.result.current.data?.balance).toBe(200_000));
    expect(calls).toEqual(["vouchers_describe"]);
  });
});
