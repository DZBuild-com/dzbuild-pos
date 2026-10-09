import { describe, expect, it } from "vitest";
import { type CheckoutLine, checkoutSteps } from "./checkout-steps";

const line = (overrides: Partial<CheckoutLine> = {}): CheckoutLine => ({
  productId: "p",
  name: "Article",
  pieces: 1,
  serials: 0,
  trackSerials: false,
  controlled: false,
  jewel: false,
  ...overrides,
});

describe("the checkout's pack steps", () => {
  it("asks the serials, then the prescriptions, then the buyer, only what is missing", () => {
    const steps = checkoutSteps({
      lines: [
        line({ productId: "p-a54", name: "Samsung A54", pieces: 2, serials: 1, trackSerials: true }),
        line({ productId: "p-tram", name: "Tramadol", controlled: true }),
        line({ productId: "p-ring", name: "Bague or 18 carats", jewel: true }),
        line({ productId: "p-otc", name: "Doliprane", controlled: true }),
      ],
      prescribed: [3],
      hasBuyer: false,
      customerIdentified: false,
    });
    expect(steps).toEqual([
      { kind: "serials", line: 0, productId: "p-a54", name: "Samsung A54", count: 2 },
      { kind: "prescriptions", lines: [{ line: 1, name: "Tramadol" }] },
      { kind: "buyer" },
    ]);
  });

  it("asks nothing for a sale that has all it needs", () => {
    expect(
      checkoutSteps({
        lines: [
          line({ trackSerials: true, pieces: 1, serials: 1 }),
          line({ jewel: true }),
          line({ productId: null, trackSerials: true }),
        ],
        prescribed: [],
        hasBuyer: false,
        customerIdentified: true,
      }),
    ).toEqual([]);
  });
});
