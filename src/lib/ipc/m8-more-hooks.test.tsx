import { renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useBaridpayQr } from "@/lib/ipc/completion";
import { useSellVoucher, useVoucherQuote, useVouchers } from "@/lib/ipc/loyalty";
import { useZakatPreview } from "@/lib/ipc/staff";
import type { SellInput, ZakatInput } from "@/lib/ipc/types";
import { mockCommands, providers } from "@/test/render";

const sale: SellInput = {
  amount: 500_000,
  method: "cash",
  reference: null,
  chequeBank: null,
  chequeDate: null,
  customerId: null,
  buyerName: "Karim",
};

const voucher = {
  id: "v1",
  code: "BA2026-000001",
  barcode: "2982600000018",
  kind: "gift",
  initialAmount: 500_000,
  balance: 500_000,
  status: "active",
  issuedOn: "2026-09-28",
  expiresOn: "2027-09-28",
  customerId: null,
  buyerName: "Karim",
  sourceDocumentId: null,
  stampDuty: 5_000,
  stampCharged: 5_000,
};

describe("M8 loyalty, voucher, zakat and completion hooks", () => {
  it("quotes a voucher from the backend and never an empty one", async () => {
    const calls = mockCommands({
      vouchers_quote_sale: () => ({
        amount: 500_000,
        stampDuty: 5_000,
        stampCharged: 5_000,
        toReceive: 505_000,
      }),
    });
    const idle = renderHook(() => useVoucherQuote({ ...sale, amount: 0 }), { wrapper: providers() });
    expect(idle.result.current.fetchStatus).toBe("idle");
    const quote = renderHook(() => useVoucherQuote(sale), { wrapper: providers() });
    await waitFor(() => expect(quote.result.current.data?.toReceive).toBe(505_000));
    expect(calls).toEqual(["vouchers_quote_sale"]);
  });

  it("refreshes the vouchers list after a voucher is sold", async () => {
    const calls = mockCommands({
      vouchers_list: () => [voucher],
      vouchers_sell: () => ({ voucher, printQueued: true }),
    });
    const filter = { kind: null, status: null, text: "" };
    const { result } = renderHook(() => ({ list: useVouchers(filter), sell: useSellVoucher() }), {
      wrapper: providers(),
    });
    await waitFor(() => expect(result.current.list.isSuccess).toBe(true));
    const done = await result.current.sell.mutateAsync(sale);
    expect(done.printQueued).toBe(true);
    await waitFor(() => expect(calls.filter((c) => c === "vouchers_list")).toHaveLength(2));
  });

  it("previews zakat only once asked, and reads the BaridiMob QR", async () => {
    const calls = mockCommands({
      zakat_preview: () => ({ belowNisab: false, zakatDue: 7_500_000 }),
      payments_baridpay_qr: () => null,
    });
    const input: ZakatInput = {
      day: null,
      valuation: "cost",
      bank: 300_000_000,
      otherAssets: 0,
      otherDebts: 0,
      note: null,
    };
    const idle = renderHook(() => useZakatPreview(null), { wrapper: providers() });
    expect(idle.result.current.fetchStatus).toBe("idle");
    const preview = renderHook(() => useZakatPreview(input), { wrapper: providers() });
    await waitFor(() => expect(preview.result.current.data?.zakatDue).toBe(7_500_000));
    const qr = renderHook(() => useBaridpayQr(), { wrapper: providers() });
    await waitFor(() => expect(qr.result.current.isSuccess).toBe(true));
    expect(qr.result.current.data).toBeNull();
    expect(calls).toEqual(["zakat_preview", "payments_baridpay_qr"]);
  });
});
