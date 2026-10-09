import { act, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { digitFromKey, PinInput, PinPad } from "@/components/pin-pad";
import { setLanguage } from "@/lib/i18n";

function Harness({ onSubmit }: { onSubmit: (pin: string) => void }) {
  const [value, setValue] = useState("");
  return <PinPad value={value} onChange={setValue} onSubmit={() => onSubmit(value)} submitLabel="OK" />;
}

function InputHarness() {
  const [value, setValue] = useState("");
  return <PinInput aria-label="pin" value={value} onChange={setValue} />;
}

describe("PinPad", () => {
  it("maps AZERTY and Arabic top rows by physical key", () => {
    expect(digitFromKey({ code: "Digit1", key: "&" })).toBe("1");
    expect(digitFromKey({ code: "Digit2", key: "é" })).toBe("2");
    expect(digitFromKey({ code: "Digit0", key: "à" })).toBe("0");
    expect(digitFromKey({ code: "Digit5", key: "٥" })).toBe("5");
    expect(digitFromKey({ code: "Numpad7", key: "7" })).toBe("7");
    expect(digitFromKey({ code: "KeyA", key: "a" })).toBeNull();
  });

  it("types, erases and submits from the keyboard", () => {
    const onSubmit = vi.fn();
    render(<Harness onSubmit={onSubmit} />);
    const pad = screen.getByRole("group", { name: "Code PIN" });
    for (const [code, key] of [
      ["Digit2", "é"],
      ["Digit5", "("],
      ["Digit8", "_"],
      ["Digit0", "à"],
      ["Digit9", "ç"],
    ]) {
      fireEvent.keyDown(pad, { code, key });
    }
    fireEvent.keyDown(pad, { code: "Backspace", key: "Backspace" });
    expect(screen.getByText("Chiffres saisis : 4")).toBeInTheDocument();
    fireEvent.keyDown(pad, { code: "Enter", key: "Enter" });
    expect(onSubmit).toHaveBeenCalledWith("2580");
  });

  it("refuses to submit fewer than four digits and stops at eight", () => {
    const onSubmit = vi.fn();
    render(<Harness onSubmit={onSubmit} />);
    const pad = screen.getByRole("group", { name: "Code PIN" });
    for (const digit of "123") fireEvent.keyDown(pad, { code: `Digit${digit}`, key: digit });
    fireEvent.keyDown(pad, { code: "Enter", key: "Enter" });
    expect(onSubmit).not.toHaveBeenCalled();
    for (const digit of "4567890") fireEvent.keyDown(pad, { code: `Digit${digit}`, key: digit });
    expect(screen.getByText("Chiffres saisis : 8")).toBeInTheDocument();
  });

  it("focuses itself on mount and again after the clear key", () => {
    render(<Harness onSubmit={vi.fn()} />);
    const pad = screen.getByRole("group", { name: "Code PIN" });
    expect(document.activeElement).toBe(pad);
    fireEvent.keyDown(pad, { code: "Digit2", key: "é" });
    act(() => pad.blur());
    expect(document.activeElement).not.toBe(pad);
    fireEvent.click(screen.getByRole("button", { name: "Tout effacer" }));
    expect(document.activeElement).toBe(pad);
    expect(screen.getByText("Chiffres saisis : 0")).toBeInTheDocument();
  });

  it("keeps the keypad in LTR order in Arabic", async () => {
    await act(async () => {
      await setLanguage("ar");
    });
    render(<Harness onSubmit={vi.fn()} />);
    expect(document.documentElement).toHaveAttribute("dir", "rtl");
    expect(screen.getByTestId("pin-keys")).toHaveAttribute("dir", "ltr");
    expect(screen.getByRole("group", { name: "الرمز السري" })).toBeInTheDocument();
  });

  it("PinInput turns AZERTY top-row keys into digits", () => {
    render(<InputHarness />);
    const input = screen.getByLabelText("pin");
    fireEvent.keyDown(input, { code: "Digit1", key: "&" });
    fireEvent.keyDown(input, { code: "Digit2", key: "é" });
    expect(input).toHaveValue("12");
    expect(input).toHaveAttribute("dir", "ltr");
  });

  it("PinInput types an AZERTY key over the selection and at the caret", () => {
    render(<InputHarness />);
    const input = screen.getByLabelText<HTMLInputElement>("pin");
    for (const [code, key] of [
      ["Digit1", "&"],
      ["Digit2", "é"],
      ["Digit3", '"'],
    ]) {
      fireEvent.keyDown(input, { code, key });
    }
    expect(input).toHaveValue("123");
    input.setSelectionRange(0, 3); // select all, then retype
    fireEvent.keyDown(input, { code: "Digit7", key: "è" });
    expect(input).toHaveValue("7");
    expect(input.selectionStart).toBe(1);
    fireEvent.keyDown(input, { code: "Digit9", key: "ç" });
    input.setSelectionRange(1, 1); // caret between 7 and 9
    fireEvent.keyDown(input, { code: "Digit4", key: "'" });
    expect(input).toHaveValue("749");
    expect([input.selectionStart, input.selectionEnd]).toEqual([2, 2]);
    // Numpad with NumLock off: the key is "Home", "End"…, the code is still the digit's.
    fireEvent.keyDown(input, { code: "Numpad1", key: "End" });
    expect(input).toHaveValue("7419");
  });

  it("PinInput stops at eight digits unless a selection is replaced", () => {
    render(<InputHarness />);
    const input = screen.getByLabelText<HTMLInputElement>("pin");
    for (const digit of "12345678") fireEvent.keyDown(input, { code: `Digit${digit}`, key: "&" });
    fireEvent.keyDown(input, { code: "Digit9", key: "ç" });
    expect(input).toHaveValue("12345678");
    input.setSelectionRange(7, 8);
    fireEvent.keyDown(input, { code: "Digit9", key: "ç" });
    expect(input).toHaveValue("12345679");
  });
});
