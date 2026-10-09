import { describe, expect, it } from "vitest";
import { formatCell } from "@/components/data-table/cells";

describe("formatCell percent", () => {
  it("puts a no-break space before % in French only, like formatRate", () => {
    expect(formatCell("percent", 1_250, "fr")).toBe("12,5\u00A0%");
    expect(formatCell("percent", 1_900, "fr")).toBe("19\u00A0%");
    expect(formatCell("percent", 1_250, "ar")).toBe("12,5%");
    expect(formatCell("percent", 1_250, "en")).toBe("12.5%");
  });
});
