import { describe, expect, it } from "vitest";
import { initials } from "./utils";

describe("initials", () => {
  it("takes the first letter of the first two words", () => {
    expect(initials("Amine Benali")).toBe("AB");
    expect(initials("ياسين")).toBe("ي");
    expect(initials("  yacine  ")).toBe("Y");
  });

  it("skips bracketed parts and anything that is not a letter or a digit", () => {
    expect(initials("Amine (démo)")).toBe("A");
    expect(initials("أمين (تجريبي)")).toBe("أ");
    expect(initials("(Promo) Lait")).toBe("L");
    expect(initials("« Karim » B.")).toBe("KB");
    expect(initials("2 frères")).toBe("2F");
    expect(initials("()")).toBe("");
  });
});
