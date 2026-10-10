import { beforeEach, describe, expect, it } from "vitest";
import { toCart, useCart } from "@/features/pos/cart-store";
import { useExtras } from "@/features/pos/extras-store";

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
const sugar = {
  productId: "p-sugar",
  qty: 1_000,
  display: { name: "Sucre 1 kg", unitCode: "pc", unitDecimals: 0, price: 9_000 },
};

/** Milk, oil and sugar on three lines, line `selected` selected. */
function threeLines(selected: number) {
  const cart = useCart.getState();
  cart.addProduct(milk);
  cart.addProduct(oil);
  cart.addProduct(sugar);
  cart.select(selected);
}

const names = () => useCart.getState().lines.map((d) => d.display.name);
const selectedName = () => {
  const { lines, selected } = useCart.getState();
  return selected === null ? null : (lines[selected]?.display.name ?? null);
};

describe("cart store", () => {
  beforeEach(() => {
    useCart.getState().clear("fr");
    useExtras.getState().clear();
  });

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

  describe("removing a line", () => {
    it("removing the first line keeps the selection on the line it was on", () => {
      threeLines(2);
      useCart.getState().remove(0);
      expect(names()).toEqual(["Huile Elio 5 L", "Sucre 1 kg"]);
      expect(selectedName()).toBe("Sucre 1 kg");
      expect(useCart.getState().selected).toBe(1);
    });

    it("removing a line after the selected one leaves the selection alone", () => {
      threeLines(0);
      useCart.getState().remove(1);
      expect(names()).toEqual(["Lait Candia 1 L", "Sucre 1 kg"]);
      expect(selectedName()).toBe("Lait Candia 1 L");
    });

    it("removing the selected middle line selects the line that takes its place", () => {
      threeLines(1);
      useCart.getState().remove(1);
      expect(names()).toEqual(["Lait Candia 1 L", "Sucre 1 kg"]);
      expect(selectedName()).toBe("Sucre 1 kg");
    });

    it("removing the selected last line selects the new last line", () => {
      threeLines(2);
      useCart.getState().remove(2);
      expect(names()).toEqual(["Lait Candia 1 L", "Huile Elio 5 L"]);
      expect(selectedName()).toBe("Huile Elio 5 L");
    });

    it("removing the only line leaves nothing selected", () => {
      useCart.getState().addProduct(milk);
      expect(useCart.getState().remove(0)?.display.name).toBe("Lait Candia 1 L");
      expect(useCart.getState().lines).toEqual([]);
      expect(useCart.getState().selected).toBeNull();
    });

    it("a line that is not there removes nothing and keeps the selection", () => {
      threeLines(1);
      expect(useCart.getState().remove(5)).toBeNull();
      expect(names()).toHaveLength(3);
      expect(useCart.getState().selected).toBe(1);
    });

    it("the later lines' serials move up with them", () => {
      threeLines(2);
      useExtras.getState().setSerials(2, ["356938035643809"]);
      useCart.getState().remove(0);
      expect(useExtras.getState().serials).toEqual({ 1: ["356938035643809"] });
    });
  });

  it("a lower quantity drops the serials of the pieces no longer sold", () => {
    useCart.getState().addProduct({ ...milk, qty: 3_000 });
    useExtras.getState().setSerials(0, ["A1", "A2", "A3"]);
    useCart.getState().setQty(0, 2_000);
    expect(useExtras.getState().serials).toEqual({ 0: ["A1", "A2"] });
    // a higher quantity keeps them: the checkout asks for the missing one
    useCart.getState().setQty(0, 4_000);
    expect(useExtras.getState().serials).toEqual({ 0: ["A1", "A2"] });
    useCart.getState().bump(0, -1);
    useCart.getState().bump(0, -1);
    expect(useExtras.getState().serials).toEqual({ 0: ["A1", "A2"] });
    useCart.getState().bump(0, -1);
    expect(useExtras.getState().serials).toEqual({ 0: ["A1"] });
  });

  it("a recalled ticket comes back with its last line selected", () => {
    const cart = useCart.getState();
    cart.addProduct(milk);
    cart.addProduct(oil);
    const held = toCart(useCart.getState());
    cart.clear("fr");
    cart.load(held, [milk.display, oil.display]);
    expect(useCart.getState().selected).toBe(1);
    cart.clear("fr");
    cart.load({ ...held, lines: [] }, []);
    expect(useCart.getState().selected).toBeNull();
  });
});
