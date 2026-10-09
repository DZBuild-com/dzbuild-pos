import { fireEvent, render, renderHook, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NumericInput } from "@/components/numeric-input";
import { OWN_SCAN, ScanDetector, decodeKey, setBurstGuard, useScanner } from "@/lib/scanner";

type Key = { code: string; key: string; shiftKey: boolean; timeStamp: number };

/** What a US-emulating scanner sends for `text`, 4 ms a key, then Enter. */
function usBurst(text: string, start = 1_000): Key[] {
  const keys: Key[] = [...text].map((ch, i) => {
    const digit = /\d/.test(ch);
    const upper = /[A-Z]/.test(ch);
    const code = digit ? `Digit${ch}` : ch === "-" ? "Minus" : `Key${ch.toUpperCase()}`;
    return { code, key: ch, shiftKey: upper, timeStamp: start + i * 4 };
  });
  return [...keys, { code: "Enter", key: "Enter", shiftKey: false, timeStamp: start + text.length * 4 }];
}

/** Dispatches `k` as a keydown carrying its own `timeStamp` (a busy parallel run would otherwise
 * spread the keys past the scanner's gaps); `true` when it was not prevented. */
function keyDown(target: Element, k: Key): boolean {
  const event = new KeyboardEvent("keydown", {
    code: k.code,
    key: k.key,
    shiftKey: k.shiftKey,
    bubbles: true,
    cancelable: true,
  });
  // the clock the page's events carry, plus the key's offset in its burst
  Object.defineProperty(event, "timeStamp", { value: new Event("probe").timeStamp + k.timeStamp - 1_000 });
  return fireEvent(target, event);
}

describe("barcode scanner", () => {
  afterEach(() => setBurstGuard(false));

  it("decodes a scanner on an AZERTY or Arabic Windows layout by physical keys", () => {
    const detector = new ScanDetector({ layout: "us" });
    // Windows AZERTY turns the digit row into & é " ' … and Arabic turns letters into Arabic ones;
    // the physical `code` stays the scanner's.
    const keys = usBurst("6130000000017").map((k, i) =>
      k.code === "Enter" ? k : { ...k, key: i % 2 ? "é" : "ض" },
    );
    const results = keys.map((k) => detector.feed(k));
    expect(results.at(-1)).toBe("6130000000017");
    expect(results.slice(0, -1).every((r) => r === null)).toBe(true);
  });

  it("decodes a Code 128 ticket number with capitals and dashes", () => {
    const detector = new ScanDetector({ layout: "us" });
    expect(
      usBurst("TK2026-C1-000123")
        .map((k) => detector.feed(k))
        .at(-1),
    ).toBe("TK2026-C1-000123");
  });

  it("decodes a scanner that emulates AZERTY", () => {
    expect(decodeKey("Digit1", true, "azerty")).toBe("1");
    expect(decodeKey("Digit6", false, "azerty")).toBe("-");
    expect(decodeKey("KeyQ", true, "azerty")).toBe("A");
    expect(decodeKey("Digit1", false, "us")).toBe("1");
    expect(decodeKey("ShiftLeft", false, "us")).toBeNull();
  });

  it("does not take a person typing for a scan", () => {
    const detector = new ScanDetector({ layout: "us" });
    const slow = usBurst("6130000000017").map((k, i) => ({ ...k, timeStamp: 1_000 + i * 120 }));
    expect(slow.map((k) => detector.feed(k)).at(-1)).toBeNull();
    const short = usBurst("12345");
    expect(short.map((k) => detector.feed(k)).at(-1)).toBeNull();
  });

  it("forgets a burst when Enter comes too late", () => {
    const detector = new ScanDetector({ layout: "us" });
    const keys = usBurst("6130000000017");
    const enter = keys.pop();
    keys.forEach((k) => detector.feed(k));
    expect(detector.feed({ ...enter!, timeStamp: enter!.timeStamp + 500 })).toBeNull();
  });

  it("calls onScan and swallows the Enter that ends the scan", () => {
    const onScan = vi.fn();
    const onClick = vi.fn();
    function Probe() {
      useScanner(onScan, { layout: "us" });
      return <button onClick={onClick}>Encaisser</button>;
    }
    render(<Probe />);
    const button = screen.getByRole("button");
    button.focus();
    for (const k of usBurst("6130000000017")) keyDown(button, k);
    expect(onScan).toHaveBeenCalledWith("6130000000017");
    expect(onScan).toHaveBeenCalledTimes(1);
    expect(onClick).not.toHaveBeenCalled();
  });

  it("a burst typed into the quantity field never changes it (HW-022)", () => {
    setBurstGuard(true);
    function Qty() {
      const [value, setValue] = useState("2");
      return <NumericInput aria-label="qty" kind="qty" value={value} onValueChange={setValue} />;
    }
    render(<Qty />);
    const input = screen.getByLabelText("qty");
    input.focus();
    for (const k of usBurst("6130000000017")) {
      // a browser inserts the character only when the keydown was not prevented
      const inserted = keyDown(input, k);
      if (inserted && k.key.length === 1)
        fireEvent.change(input, { target: { value: `${(input as HTMLInputElement).value}${k.key}` } });
    }
    // the first key of the burst landed, and was taken back when the second arrived
    expect(input).toHaveValue("2");
  });

  it("leaves the scans typed in a field that reads its own (serial numbers) to that field", () => {
    const onScan = vi.fn();
    function Probe() {
      useScanner(onScan, { layout: "us" });
      return (
        <>
          <input aria-label="imei" {...OWN_SCAN} />
          <input aria-label="coût" />
        </>
      );
    }
    render(<Probe />);
    for (const k of usBurst("356938035643809")) keyDown(screen.getByLabelText("imei"), k);
    expect(onScan).not.toHaveBeenCalled();
    for (const k of usBurst("6130000000017")) keyDown(screen.getByLabelText("coût"), k);
    expect(onScan).toHaveBeenCalledWith("6130000000017");
  });

  it("removes the scanner's prefix and ends a silent scan after a pause", () => {
    vi.useFakeTimers();
    const onScan = vi.fn();
    renderHook(() => useScanner(onScan, { layout: "us", prefix: "]E0", suffix: "silence" }));
    const t0 = performance.now();
    [..."]E06130123456782"].forEach((ch, i) => {
      const code = ch === "]" ? "BracketRight" : ch === "E" ? "KeyE" : `Digit${ch}`;
      window.dispatchEvent(
        new KeyboardEvent("keydown", { code, key: ch, shiftKey: ch === "E", bubbles: true }),
      );
      vi.setSystemTime(t0 + (i + 1) * 5);
    });
    vi.advanceTimersByTime(100);
    expect(onScan).toHaveBeenCalledWith("6130123456782");
    vi.useRealTimers();
  });
});
