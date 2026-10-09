import { describe, expect, it } from "vitest";
import { toAsciiDigits } from "@/lib/digits";

describe("toAsciiDigits", () => {
  it("turns Arabic-Indic digits into ASCII digits", () => {
    expect(toAsciiDigits("٠١٢٣٤٥٦٧٨٩")).toBe("0123456789");
    expect(toAsciiDigits("٠٥٥٥ ١٢ ٣٤ ٥٦")).toBe("0555 12 34 56");
  });

  it("keeps every other character as typed", () => {
    expect(toAsciiDigits("+213 (0) 555-12")).toBe("+213 (0) 555-12");
    expect(toAsciiDigits("٠٥ab٩")).toBe("05ab9");
    expect(toAsciiDigits("")).toBe("");
  });
});
