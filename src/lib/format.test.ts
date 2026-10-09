import { describe, expect, it } from "vitest";
import {
  formatAmountInput,
  formatDate,
  formatNumber,
  formatDZD,
  formatPercentInput,
  parseDZD,
  parsePercentBp,
} from "@/lib/format";
import { localizedName } from "@/lib/names";

// Expected strings spell invisible characters as escapes: U+202F NARROW NO-BREAK SPACE (thousands
// separator in fr/ar), U+00A0 NO-BREAK SPACE (before DA / دج), U+200E/U+200F LRM/RLM (bidi marks).
describe("formatDZD (spec §11)", () => {
  it("prints the three reference outputs exactly", () => {
    expect(formatDZD(450_000, "fr")).toBe("4\u202F500,00\u00A0DA");
    expect(formatDZD(450_000, "en")).toBe("4,500.00\u00A0DA");
    expect(formatDZD(450_000, "ar")).toBe("4\u202F500,00\u00A0دج");
  });

  it("handles zero, negatives, millions and single centimes", () => {
    expect(formatDZD(0, "fr")).toBe("0,00\u00A0DA");
    expect(formatDZD(-5, "fr")).toBe("-0,05\u00A0DA");
    expect(formatDZD(100_000_005, "en")).toBe("1,000,000.05\u00A0DA");
  });

  it("hides centimes by rounding half away from zero", () => {
    expect(formatDZD(450_050, "fr", { hideCentimes: true })).toBe("4\u202F501\u00A0DA");
    expect(formatDZD(-450_050, "fr", { hideCentimes: true })).toBe("-4\u202F501\u00A0DA");
    expect(formatDZD(450_049, "en", { hideCentimes: true })).toBe("4,500\u00A0DA");
  });

  it("refuses non-integer amounts", () => {
    expect(() => formatDZD(1.5, "fr")).toThrow(RangeError);
  });
});

describe("parsing typed amounts", () => {
  it("reads French, English and Arabic habits", () => {
    expect(parseDZD("4 500,00")).toBe(450_000);
    expect(parseDZD("4,500.00")).toBe(450_000);
    expect(parseDZD("1 000 000,5")).toBe(100_000_050);
    expect(parseDZD("1,000")).toBe(100_000);
    expect(parseDZD("4500 DA")).toBe(450_000);
    expect(parseDZD("12,345")).toBe(1_234_500);
  });

  it("rejects garbage and negatives", () => {
    for (const bad of ["", "abc", "-5", "12a,50", "5 DA 5"]) expect(parseDZD(bad)).toBeNull();
  });

  it("reads and prints percentages as basis points", () => {
    expect(parsePercentBp("5")).toBe(500);
    expect(parsePercentBp("5,5")).toBe(550);
    expect(parsePercentBp("12.25")).toBe(1_225);
    expect(parsePercentBp("100")).toBe(10_000);
    expect(parsePercentBp("101")).toBeNull();
    expect(parsePercentBp("x")).toBeNull();
    expect(formatPercentInput(550, "fr")).toBe("5,5");
    expect(formatPercentInput(1_225, "en")).toBe("12.25");
    expect(formatPercentInput(500, "ar")).toBe("5");
  });

  it("prints amounts for editing and reads them back unchanged", () => {
    expect(formatAmountInput(100_000_050, "fr")).toBe("1000000,50");
    expect(formatAmountInput(100_000_050, "en")).toBe("1000000.50");
    expect(formatAmountInput(0, "ar")).toBe("0,00");
    for (const lang of ["ar", "fr", "en"] as const)
      expect(parseDZD(formatAmountInput(123_456, lang))).toBe(123_456);
    expect(() => formatAmountInput(0.5, "fr")).toThrow(RangeError);
  });
});

describe("formatDate", () => {
  const NOON_UTC = 1_790_596_800_000; // 2026-09-28T12:00:00Z = 13:00 in Algiers

  it("uses the Gregorian calendar in Algiers time with Western digits", () => {
    expect(formatDate(NOON_UTC, "fr")).toBe("28/09/2026");
    expect(formatDate(NOON_UTC, "fr", { withTime: true })).toBe("28/09/2026 13:00");
    expect(formatDate(1_798_759_800_000, "fr")).toBe("01/01/2027");
    expect(formatDate(NOON_UTC, "en", { withTime: true })).toBe("28/09/2026, 13:00");
  });

  it("prints Arabic dates without bidi marks, so an LTR <bdi> keeps their order", () => {
    // Raw output, no clean-up in the test: Intl's ar-DZ marks (U+200E, U+200F, U+061C) reorder
    // the characters inside <bdi dir="ltr">, and its Arabic comma (U+060C) is not our separator.
    expect(formatDate(NOON_UTC, "ar")).toBe("28/09/2026");
    expect(formatDate(NOON_UTC, "ar", { withTime: true })).toBe("28/09/2026 13:00");
    expect(formatDate(NOON_UTC, "ar", { withTime: true })).not.toMatch(
      /[\u200E\u200F\u061C\u060C\u0660-\u0669]/,
    );
  });

  it("prints numbers without bidi marks", () => {
    expect(formatNumber(-1.5, "ar", 1)).not.toMatch(/[\u200E\u200F\u061C\u0660-\u0669]/);
    expect(formatNumber(0.5, "fr", 1)).toBe("0,5");
  });
});

describe("localizedName", () => {
  it("prefers the UI language, then fr, ar, en", () => {
    const role = { nameAr: "المالك", nameFr: "Propriétaire", nameEn: "" };
    expect(localizedName(role, "ar")).toBe("المالك");
    expect(localizedName(role, "en")).toBe("Propriétaire");
    expect(localizedName({ nameAr: "مغازة", nameFr: " ", nameEn: "" }, "fr")).toBe("مغازة");
  });
});
