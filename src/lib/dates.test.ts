import { describe, expect, it } from "vitest";
import { rangeOf } from "@/lib/dates";

describe("rangeOf", () => {
  it("follows the Algerian week and calendar months", () => {
    expect(rangeOf("this_week", "2026-09-30")).toEqual({ from: "2026-09-27", to: "2026-09-30" });
    expect(rangeOf("last_week", "2026-09-30")).toEqual({ from: "2026-09-20", to: "2026-09-26" });
    expect(rangeOf("last_month", "2026-03-31")).toEqual({ from: "2026-02-01", to: "2026-02-28" });
    expect(rangeOf("this_year", "2026-09-30")).toEqual({ from: "2026-01-01", to: "2026-09-30" });
  });
});
