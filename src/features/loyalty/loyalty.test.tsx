import { clearMocks } from "@tauri-apps/api/mocks";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { loyaltyAccount, packState, sessionWith, voucher } from "@/test/fixtures-m8";
import { LoyaltyTab } from "./LoyaltyTab";
import { SellVoucherDialog } from "./SellVoucherDialog";

afterEach(() => clearMocks());
const cashier = sessionWith(["sales.create", "customers.view"]);

describe("loyalty and vouchers", () => {
  it("redeems points into a loyalty voucher and never offers a draw", async () => {
    const redeemed: unknown[] = [];
    mockCommands({
      auth_session: () => cashier,
      packs_state: () => packState({ modules: ["loyalty"] }),
      loyalty_account: () => loyaltyAccount({ balance: 640, redeemable: true }),
      loyalty_entries: () => [],
      loyalty_redeem: (args) => {
        redeemed.push(args);
        return { voucher: voucher({ kind: "loyalty", initialAmount: 50_000 }), printQueued: true };
      },
    });
    const user = userEvent.setup();
    await renderM8(<LoyaltyTab customerId="c1" />);
    expect(await screen.findByText("640 points")).toBeInTheDocument();
    expect(screen.queryByText(/tirage|tombola|loterie/i)).toBeNull();
    await user.click(screen.getByRole("button", { name: "Échanger des points" }));
    await user.clear(screen.getByLabelText("Points à échanger"));
    await user.type(screen.getByLabelText("Points à échanger"), "500");
    expect(screen.getByText("Chèque fidélité de 500,00 DA")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Créer le chèque fidélité" }));
    await waitFor(() => expect(redeemed[0]).toMatchObject({ customerId: "c1", points: 500 }));
  });

  it("sells a gift voucher with its duty shown", async () => {
    const sold: unknown[] = [];
    mockCommands({
      auth_session: () => cashier,
      packs_state: () => packState({ modules: ["gift_vouchers"] }),
      vouchers_quote_sale: () => ({
        amount: 500_000,
        stampDuty: 5_000,
        stampCharged: 5_000,
        toReceive: 505_000,
      }),
      vouchers_sell: (args) => {
        sold.push(args);
        return { voucher: voucher({}), printQueued: true };
      },
    });
    const user = userEvent.setup();
    await renderM8(<SellVoucherDialog onClose={() => {}} />);
    // The quick amount reads as on screen: French groups thousands with U+202F (M0's formatter).
    await user.click(await screen.findByRole("button", { name: "5\u202F000 DA" }));
    expect(await screen.findByText("À encaisser : 5 050,00 DA (timbre 50,00 DA)")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Encaisser et imprimer" }));
    await waitFor(() => expect(sold[0]).toMatchObject({ input: { amount: 500_000, method: "cash" } }));
  });
});
