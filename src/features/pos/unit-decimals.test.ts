import { describe, expect, it } from "vitest";
import { unitDecimalsOf } from "./unit-decimals";

const units = [
  { code: "pc", decimals: 0 },
  { code: "kg", decimals: 3 },
  { code: "ctn", decimals: 0 },
  { code: "m", decimals: 2 },
];

describe("a till quantity's decimals", () => {
  it("are the product's own when it carries them (a scan)", () => {
    expect(unitDecimalsOf({ unitCode: "kg", unitDecimals: 2 }, units)).toBe(2);
    expect(unitDecimalsOf({ unitCode: "pc", unitDecimals: 0 })).toBe(0);
  });

  it("are its unit's for a search result or a tile", () => {
    expect(unitDecimalsOf({ unitCode: "ctn" }, units)).toBe(0);
    expect(unitDecimalsOf({ unitCode: "m" }, units)).toBe(2);
  });

  it("before the units are read: pieces whole, other units to the thousandth", () => {
    expect(unitDecimalsOf({ unitCode: "pc" })).toBe(0);
    expect(unitDecimalsOf({ unitCode: "kg" })).toBe(3);
    expect(unitDecimalsOf({ unitCode: "l" }, units)).toBe(3);
  });
});
