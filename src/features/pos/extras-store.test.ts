import { afterEach, describe, expect, it } from "vitest";
import { useCart } from "./cart-store";
import { useExtras } from "./extras-store";

const display = { name: "Samsung A54", unitCode: "pc", unitDecimals: 0, price: 6_000_000 };

afterEach(() => {
  useCart.getState().clear("fr");
  useExtras.getState().clear();
  useExtras.getState().setVan(null);
});

describe("the sale's pack extras", () => {
  it("follow the cart's lines when one is removed", () => {
    const cart = useCart.getState();
    cart.addProduct({ productId: "p-a", qty: 1_000, display });
    cart.addProduct({ productId: "p-b", qty: 1_000, display });
    cart.addProduct({ productId: "p-c", qty: 2_000, display });
    const extras = useExtras.getState();
    extras.setSerials(0, ["356938035643809"]);
    extras.setSerials(2, ["490154203237518", "353918057112230"]);
    extras.setPrescriptions([
      {
        line: 2,
        prescriberName: "Dr Meziane",
        prescriberRef: null,
        prescriptionNo: null,
        prescriptionDate: "2026-10-02",
        patientName: "M. Haddad",
        patientRef: null,
      },
    ]);
    useCart.getState().remove(1);
    expect(useExtras.getState().serials).toEqual({
      0: ["356938035643809"],
      1: ["490154203237518", "353918057112230"],
    });
    expect(useExtras.getState().prescriptions[0]?.line).toBe(1);
    expect(useExtras.getState().toExtras().serials).toEqual([
      { line: 0, serialNo: "356938035643809" },
      { line: 1, serialNo: "490154203237518" },
      { line: 1, serialNo: "353918057112230" },
    ]);
  });

  it("are cleared with the cart, but a van sale lasts until the till goes back to the shop", () => {
    const extras = useExtras.getState();
    extras.setSerials(0, ["356938035643809"]);
    extras.setBuyer({ name: "Nadia Ouali", idKind: "cni", idNumber: "109870123", address: "Oran" });
    extras.setVan({ locationId: "loc-van-1", name: "Camion 1", round: "TR2026-000004" });
    useCart.getState().clear("fr");
    expect(useExtras.getState().toExtras()).toEqual({
      serials: [],
      prescriptions: [],
      buyer: undefined,
      paymentTypes: [],
      locationId: "loc-van-1",
    });
  });

  it("send nothing for a sale without any", () => {
    expect(useExtras.getState().toExtras()).toEqual({
      serials: [],
      prescriptions: [],
      buyer: undefined,
      paymentTypes: [],
      locationId: undefined,
    });
  });
});
