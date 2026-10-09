import { beforeEach, describe, expect, it } from "vitest";
import { toCart, useCart } from "@/features/pos/cart-store";

const display = { name: "Tomates", unitCode: "kg", unitDecimals: 3, price: 12_000, latinName: "Tomates" };

describe("a held ticket's weighed packages", () => {
  beforeEach(() => useCart.getState().clear("fr"));

  it("go to the backend as packages and come back as lines no scan merges into", () => {
    const cart = useCart.getState();
    cart.addProduct({ productId: "p-tomato", qty: 1_750, display, separate: true });
    cart.addProduct({ productId: "p-tomato", qty: 500, display });
    // what F6 holds (`sales_hold`)
    const held = toCart(useCart.getState());
    expect(held.lines.map((l) => [l.qty, l.separate ?? false])).toEqual([
      [1_750, true],
      [500, false],
    ]);
    // F7 brings it back (`sales_recall`), then the cashier weighs more of the same
    useCart.getState().clear("fr");
    useCart.getState().load(held, [display, display]);
    useCart.getState().addProduct({ productId: "p-tomato", qty: 250, display });
    expect(useCart.getState().lines.map((d) => d.line.qty)).toEqual([1_750, 750]);
    useCart.getState().addProduct({ productId: "p-tomato", qty: 1_250, display, separate: true });
    expect(useCart.getState().lines.map((d) => d.line.qty)).toEqual([1_750, 750, 1_250]);
  });
});
