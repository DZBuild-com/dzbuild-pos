import { beforeEach, describe, expect, it } from "vitest";
import { toCart, useCart } from "@/features/pos/cart-store";

const milk = {
  productId: "p-milk",
  qty: 1_000,
  display: { name: "Lait Candia 1 L", unitCode: "pc", unitDecimals: 0, price: 15_000 },
};
const oil = {
  productId: "p-oil",
  qty: 1_000,
  display: { name: "Huile Elio 5 L", unitCode: "pc", unitDecimals: 0, price: 65_000 },
};

describe("cart store", () => {
  beforeEach(() => useCart.getState().clear("fr"));

  it("merges repeated scans and selects the line", () => {
    const cart = useCart.getState();
    cart.addProduct(milk);
    cart.addProduct(oil);
    cart.addProduct(milk);
    const { lines, selected, lastScan } = useCart.getState();
    expect(lines.map((d) => [d.line.productId, d.line.qty])).toEqual([
      ["p-milk", 2_000],
      ["p-oil", 1_000],
    ]);
    expect(selected).toBe(0);
    expect(lastScan?.name).toBe("Lait Candia 1 L");
  });

  it("applies the * multiplier to the next product only", () => {
    const cart = useCart.getState();
    cart.setMultiplier(3_000);
    cart.addProduct(milk);
    cart.addProduct(oil);
    expect(useCart.getState().lines.map((d) => d.line.qty)).toEqual([3_000, 1_000]);
    expect(useCart.getState().multiplier).toBeNull();
  });

  it("changes quantities with + and - but never below one unit", () => {
    const cart = useCart.getState();
    cart.addProduct(milk);
    cart.bump(0, 1);
    cart.bump(0, -1);
    cart.bump(0, -1);
    expect(useCart.getState().lines[0]?.line.qty).toBe(1_000);
  });

  it("keeps a selection after removing a line and moves it with the arrows", () => {
    const cart = useCart.getState();
    cart.addProduct(milk);
    cart.addProduct(oil);
    const removed = cart.remove(1);
    expect(removed?.display.name).toBe("Huile Elio 5 L");
    expect(useCart.getState().selected).toBe(0);
    cart.addProduct(oil);
    cart.select(0);
    cart.move(1);
    cart.move(1);
    expect(useCart.getState().selected).toBe(1);
  });

  it("a line with a discount or a price is no longer merged", () => {
    const cart = useCart.getState();
    cart.addProduct(milk);
    cart.setDiscount(0, { kind: "percent", value: 1_000 });
    cart.addProduct(milk);
    expect(useCart.getState().lines).toHaveLength(2);
  });

  it("builds the Cart the backend prices", () => {
    const cart = useCart.getState();
    cart.setLang("ar");
    cart.addProduct(milk);
    cart.addOpenItem({ description: "Pain", unitPrice: 1_500, taxRateId: "tr-9", qty: 2_000 });
    expect(toCart(useCart.getState())).toEqual({
      lines: [
        {
          productId: "p-milk",
          description: null,
          qty: 1_000,
          unitPrice: null,
          discount: { kind: "none" },
          taxRateId: null,
          note: null,
          expectedUnitPrice: 15_000,
        },
        {
          productId: null,
          description: "Pain",
          qty: 2_000,
          unitPrice: 1_500,
          discount: { kind: "none" },
          taxRateId: "tr-9",
          note: null,
          expectedUnitPrice: null,
        },
      ],
      priceLevelId: null,
      customerId: null,
      lang: "ar",
      note: null,
    });
  });

  it("repeats the last line (SAL-029)", () => {
    const cart = useCart.getState();
    cart.addProduct(oil);
    cart.addProduct(milk);
    cart.repeatLast();
    expect(useCart.getState().lines.map((d) => d.line.qty)).toEqual([1_000, 2_000]);
  });

  it("keeps weighed packages on their own lines, never merged into", () => {
    const display = { name: "Tomates", unitCode: "kg", unitDecimals: 3, price: 12_000, latinName: "Tomates" };
    useCart.getState().clear("fr");
    useCart.getState().addProduct({ productId: "p-tomato", qty: 1_750, display, separate: true });
    useCart.getState().addProduct({ productId: "p-tomato", qty: 1_250, display, separate: true });
    expect(useCart.getState().lines.map((l) => l.line.qty)).toEqual([1_750, 1_250]);
    useCart.getState().addProduct({ productId: "p-tomato", qty: 500, display });
    useCart.getState().addProduct({ productId: "p-tomato", qty: 500, display });
    expect(useCart.getState().lines.map((l) => l.line.qty)).toEqual([1_750, 1_250, 1_000]);
  });

  it("repeats a weighed package as a package of its own (F10)", () => {
    const display = { name: "Tomates", unitCode: "kg", unitDecimals: 3, price: 12_000, latinName: "Tomates" };
    useCart.getState().addProduct({ productId: "p-tomato", qty: 1_750, display, separate: true });
    useCart.getState().repeatLast();
    expect(useCart.getState().lines.map((l) => l.line.qty)).toEqual([1_750, 1_750]);
  });
});
