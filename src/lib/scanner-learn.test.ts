import { describe, expect, it } from "vitest";
import type { ScanKey } from "@/lib/scanner";
import { learnScanner } from "@/lib/scanner-learn";

/** The keys a scanner sends to type `text` when it emulates `layout` (inverse of decodeKey). */
function keysFor(text: string, layout: "us" | "azerty", end: "Enter" | null): ScanKey[] {
  const US: Record<string, [string, boolean]> = {};
  for (const d of "0123456789") US[d] = [`Digit${d}`, false];
  for (const l of "ABCDEFGHIJKLMNOPQRSTUVWXYZ") {
    US[l] = [`Key${l}`, true];
    US[l.toLowerCase()] = [`Key${l}`, false];
  }
  US["-"] = ["Minus", false];
  US["]"] = ["BracketRight", false];
  const AZ: Record<string, [string, boolean]> = { ...US };
  for (const d of "0123456789") AZ[d] = [`Digit${d}`, true];
  AZ["A"] = ["KeyQ", true];
  AZ["Q"] = ["KeyA", true];
  AZ["Z"] = ["KeyW", true];
  AZ["W"] = ["KeyZ", true];
  AZ["-"] = ["Digit6", false];
  const map = layout === "us" ? US : AZ;
  const keys: ScanKey[] = [...text].map((ch, i) => {
    const [code, shiftKey] = map[ch]!;
    return { code, key: ch, shiftKey, timeStamp: i * 8 };
  });
  if (end) keys.push({ code: "Enter", key: "Enter", shiftKey: false, timeStamp: text.length * 8 });
  return keys;
}

describe("learnScanner", () => {
  it("finds a US scanner that ends with Enter", () => {
    expect(learnScanner(keysFor("DZPOS-TEST-0123", "us", "Enter"), "DZPOS-TEST-0123")).toEqual({
      layout: "us",
      prefix: "",
      suffix: "enter_or_tab",
    });
  });

  it("finds an AZERTY scanner with an AIM prefix and no suffix", () => {
    expect(learnScanner(keysFor("]E0DZPOS-TEST-0123", "azerty", null), "DZPOS-TEST-0123")).toEqual({
      layout: "azerty",
      prefix: "]E0",
      suffix: "silence",
    });
  });

  it("gives up on a scan that never contains the expected code", () => {
    expect(learnScanner(keysFor("6130123456782", "us", "Enter"), "DZPOS-TEST-0123")).toBeNull();
  });
});
