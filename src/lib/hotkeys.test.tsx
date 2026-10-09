import { fireEvent, render, renderHook } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Dialog } from "@/components/ui/dialog";
import { useHotkeys } from "@/lib/hotkeys";

function Probe({ onPay, dialogOpen }: { onPay: () => void; dialogOpen: boolean }) {
  useHotkeys({ F12: onPay });
  return (
    <Dialog open={dialogOpen} onOpenChange={() => undefined} title="Remise">
      <p>…</p>
    </Dialog>
  );
}

describe("useHotkeys", () => {
  it("runs the action and stops the browser's own F12", () => {
    const onPay = vi.fn();
    render(<Probe onPay={onPay} dialogOpen={false} />);
    const event = new KeyboardEvent("keydown", { key: "F12", cancelable: true, bubbles: true });
    window.dispatchEvent(event);
    expect(onPay).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(true);
  });

  it("fires Ctrl+digit keys and ignores Alt", async () => {
    const fired: string[] = [];
    renderHook(() => useHotkeys({ "Ctrl+2": () => fired.push("flexy"), F12: () => fired.push("pay") }));
    await userEvent.keyboard("{Control>}2{/Control}");
    await userEvent.keyboard("{Alt>}2{/Alt}");
    await userEvent.keyboard("{F12}");
    expect(fired).toEqual(["flexy", "pay"]);
  });

  it("leaves the keyboard to an open dialog", () => {
    const onPay = vi.fn();
    render(<Probe onPay={onPay} dialogOpen />);
    fireEvent.keyDown(window, { key: "F12" });
    expect(onPay).not.toHaveBeenCalled();
  });
});
