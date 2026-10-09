import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderM8 } from "@/test/render-m8";
import { sessionWith, voucher } from "@/test/fixtures-m8";
import { VouchersList } from "./VouchersList";

beforeEach(() => {
  // jsdom has no layout: TanStack Virtual shows no row at 0×0 (see customers.test.tsx).
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

/** The vouchers report as of today: 3 500 DA still owed (its total's balance column). */
function report() {
  return {
    report: {
      kind: "gift_vouchers",
      from: "2026-09-28",
      to: "2026-09-28",
      table: {
        columns: [
          "code",
          "voucherKind",
          "issuedOn",
          "expiresOn",
          "initialAmount",
          "spent",
          "balance",
          "voucherStatus",
        ].map((key) => ({ key, kind: "text" })),
        rows: [],
        totals: [null, null, null, null, 500_000, 150_000, 350_000, null],
      },
      chart: null,
      truncated: false,
    },
    title: "",
    subtitle: "",
    labels: [],
    tagLabels: {},
  };
}

describe("the vouchers list", () => {
  it("shows what the holders are owed from the report and reprints a voucher", async () => {
    const filters: unknown[] = [];
    const reprinted: unknown[] = [];
    await renderM8(<VouchersList canReprint />, {
      session: sessionWith(["reports.cash", "sales.reprint"]),
      commands: {
        vouchers_list: (args) => {
          filters.push(args.filter);
          return [voucher({ balance: 350_000, buyerName: "Karim" })];
        },
        reports_run: () => report(),
        vouchers_reprint: (args) => {
          reprinted.push(args);
          return { voucher: voucher({}), printQueued: true };
        },
      },
    });
    const user = userEvent.setup();
    expect(await screen.findByText("Reste dû aux porteurs : 3 500,00 DA")).toBeInTheDocument();
    expect(await screen.findByText("Karim")).toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("État"), "active");
    await waitFor(() => expect(filters.at(-1)).toEqual({ kind: null, status: "active", text: "" }));
    await user.click(screen.getByRole("button", { name: "Réimprimer BA2026-000001" }));
    await waitFor(() => expect(reprinted).toEqual([{ code: "BA2026-000001" }]));
  });
});
