import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { keyMap } from "@/test/fixtures-m8";
import { DEFAULT_KEYS, hotkeyOf } from "./key-map";

const TILL_RS = path.resolve(import.meta.dirname, "../../crates/pos-core/src/till.rs");

/** The backend's `till::DEFAULT_KEYS` pairs, `null` when the file does not declare them. */
function backendDefaults(source: string): Array<[string, string]> | null {
  const block = /pub const DEFAULT_KEYS: \[\(&str, &str\); \d+\] = \[([\s\S]*?)\];/.exec(source);
  if (!block?.[1]) return null;
  return [...block[1].matchAll(/\("([a-z_]+)",\s*"([^"]+)"\)/g)].map((m) => [m[1] ?? "", m[2] ?? ""]);
}

const tillRs = readFileSync(TILL_RS, "utf8");
// Task 17 (`till::{KEY_ACTIONS, DEFAULT_KEYS, key_map}`) may be missing: the comparison
// runs once it is there, and fails if the key map is there but its defaults cannot be read.
const backendHasKeyMap = /\b(?:KEY_ACTIONS|DEFAULT_KEYS|fn key_map)\b/.test(tillRs);

/** The keydown a till key comes from: Ctrl + the digit key of the top row, or the key alone. */
function press(key: string): KeyboardEvent {
  const digit = /^Ctrl\+(\d)$/.exec(key)?.[1];
  return digit === undefined
    ? new KeyboardEvent("keydown", { key })
    : new KeyboardEvent("keydown", { key: digit, code: `Digit${digit}`, ctrlKey: true });
}

describe("the till's default keys", () => {
  it.skipIf(!backendHasKeyMap)("are the backend's till::DEFAULT_KEYS, in its order", () => {
    const backend = backendDefaults(tillRs);
    expect(backend, "till::DEFAULT_KEYS could not be read from till.rs").not.toBeNull();
    expect(Object.entries(DEFAULT_KEYS)).toEqual(backend);
  });

  it("are what the mocked till_key_map answers with no change", () => {
    expect(Object.entries(DEFAULT_KEYS)).toEqual(keyMap());
  });

  it("give 17 actions 17 different keys, each one a press can give", () => {
    const keys = Object.values(DEFAULT_KEYS);
    expect(keys).toHaveLength(17);
    expect(new Set(keys).size).toBe(17);
    for (const key of keys) expect(hotkeyOf(press(key)), key).toBe(key);
  });
});

describe("hotkeyOf", () => {
  it("reads F1 to F12 but F11, Escape and Ctrl + a digit by its physical key", () => {
    expect(hotkeyOf(new KeyboardEvent("keydown", { key: "F10" }))).toBe("F10");
    expect(hotkeyOf(new KeyboardEvent("keydown", { key: "F12" }))).toBe("F12");
    expect(hotkeyOf(new KeyboardEvent("keydown", { key: "Escape" }))).toBe("Escape");
    // AZERTY: Ctrl + the "&" key is Ctrl+1; an Arabic layout types its own digit
    expect(hotkeyOf(new KeyboardEvent("keydown", { key: "&", code: "Digit1", ctrlKey: true }))).toBe(
      "Ctrl+1",
    );
    expect(hotkeyOf(new KeyboardEvent("keydown", { key: "٨", code: "Digit8", ctrlKey: true }))).toBe(
      "Ctrl+8",
    );
    expect(hotkeyOf(new KeyboardEvent("keydown", { key: "0", code: "Numpad0", ctrlKey: true }))).toBe(
      "Ctrl+0",
    );
  });

  it("gives no key for F11 (the scale panel), a letter or a press with Shift, Alt or Meta", () => {
    for (const init of [
      { key: "F11" },
      { key: "a" },
      { key: "F13" },
      { key: "F2", shiftKey: true },
      { key: "F4", altKey: true },
      { key: "F1", metaKey: true },
      { key: "!", code: "Digit1", ctrlKey: true, shiftKey: true },
      { key: "s", code: "KeyS", ctrlKey: true },
    ]) {
      expect(hotkeyOf(new KeyboardEvent("keydown", init)), JSON.stringify(init)).toBeNull();
    }
  });
});
