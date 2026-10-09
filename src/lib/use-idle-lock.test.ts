import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useIdleLock } from "@/lib/use-idle-lock";

describe("useIdleLock", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("locks after the configured minutes without activity", () => {
    vi.useFakeTimers();
    const onLock = vi.fn();
    renderHook(() => useIdleLock(2, onLock));
    vi.advanceTimersByTime(119_000);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "a" }));
    vi.advanceTimersByTime(119_000);
    expect(onLock).not.toHaveBeenCalled();
    vi.advanceTimersByTime(2_000);
    expect(onLock).toHaveBeenCalledTimes(1);
  });

  it("never locks when the setting is 0", () => {
    vi.useFakeTimers();
    const onLock = vi.fn();
    renderHook(() => useIdleLock(0, onLock));
    vi.advanceTimersByTime(24 * 60 * 60 * 1_000);
    expect(onLock).not.toHaveBeenCalled();
  });
});
