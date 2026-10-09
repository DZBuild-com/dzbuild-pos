import { describe, expect, it } from "vitest";
import { formatQty, formatRate, normalizeDigits, parseAmount, parseQty, parseQtyOrZero } from "@/lib/numeric";

describe("numeric input", () => {
  it("accepts Arabic-Indic and Persian digits (UX-014)", () => {
    expect(normalizeDigits("١٢٣٤٥٦٧٨٩٠")).toBe("1234567890");
    expect(normalizeDigits("۴۵۰")).toBe("450");
    expect(normalizeDigits("١٬٥٠٠٫٥٠")).toBe("1500,50");
  });

  it("parses amounts as integer centimes", () => {
    expect(parseAmount("1500")).toBe(150_000);
    expect(parseAmount("١٥٠٠٫٥")).toBe(150_050);
    expect(parseAmount("12,345")).toBe(1_234_500);
    expect(parseAmount("abc")).toBeNull();
  });

  it("parses quantities as integer milli-units within the unit's decimals", () => {
    expect(parseQty("2", 0)).toBe(2_000);
    expect(parseQty("1,250", 3)).toBe(1_250);
    expect(parseQty("0.5", 3)).toBe(500);
    expect(parseQty("1.5", 0)).toBeNull();
    expect(parseQty("0", 0)).toBeNull();
    expect(parseQty("٣", 0)).toBe(3_000);
    expect(parseQty("1.2345", 3)).toBeNull();
  });

  it("reads a counted or minimum quantity, zero included", () => {
    expect(parseQtyOrZero("0", 0)).toBe(0);
    expect(parseQtyOrZero("0,000", 3)).toBe(0);
    expect(parseQtyOrZero("٠", 0)).toBe(0);
    expect(parseQtyOrZero("12", 0)).toBe(12_000);
    expect(parseQtyOrZero("1,5", 0)).toBeNull();
    expect(parseQtyOrZero("", 0)).toBeNull();
    expect(parseQtyOrZero("-1", 0)).toBeNull();
  });

  it("formats quantities and rates per language", () => {
    expect(formatQty(2_000, "fr")).toBe("2");
    expect(formatQty(1_250, "fr")).toBe("1,25");
    expect(formatQty(1_250, "en")).toBe("1.25");
    expect(formatRate(1_900, "fr")).toBe("19\u00A0%");
    expect(formatRate(950, "ar")).toBe("9,5%");
    expect(formatRate(1_900, "en")).toBe("19%");
  });
});
