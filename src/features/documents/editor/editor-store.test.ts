import { describe, expect, it } from "vitest";
import { createEditorStore, toInput } from "./editor-store";

describe("editor store", () => {
  it("adds a product twice as one line and builds the draft input", () => {
    const store = createEditorStore("invoice", "fr");
    const s = store.getState();
    s.addProduct({ productId: "p-cement", qty: 1_000, name: "Ciment 25 kg", unitCode: "sac" });
    store.getState().addProduct({ productId: "p-cement", qty: 2_000, name: "Ciment 25 kg", unitCode: "sac" });
    store.getState().setCustomer("c-amine");
    store.getState().setSecondLang("ar");
    store.getState().setDueDate("2026-10-28");
    const input = toInput(store.getState());
    expect(input.docType).toBe("invoice");
    expect(input.cart.lines).toHaveLength(1);
    expect(input.cart.lines[0]).toMatchObject({ productId: "p-cement", qty: 3_000, unitPrice: null });
    expect(input.cart.customerId).toBe("c-amine");
    expect(input).toMatchObject({ secondLang: "ar", dueDate: "2026-10-28", validUntil: null });
  });

  it("never keeps a second language equal to the first", () => {
    const store = createEditorStore("quote", "ar");
    store.getState().setSecondLang("ar");
    expect(store.getState().secondLang).toBeNull();
    store.getState().setSecondLang("fr");
    store.getState().setLang("fr");
    expect(store.getState().secondLang).toBeNull();
  });

  it("keeps an overridden line apart from the catalogue one", () => {
    const store = createEditorStore("quote", "fr");
    store.getState().addProduct({ productId: "p-cement", qty: 1_000, name: "Ciment 25 kg", unitCode: "sac" });
    const key = store.getState().lines[0]!.key;
    store.getState().setUnitPrice(key, 100_000);
    store.getState().addProduct({ productId: "p-cement", qty: 1_000, name: "Ciment 25 kg", unitCode: "sac" });
    expect(store.getState().lines).toHaveLength(2);
  });
});
