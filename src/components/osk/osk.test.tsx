import { act, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NumericInput } from "@/components/numeric-input";
import { OnScreenKeyboard } from "@/components/osk/OnScreenKeyboard";
import { eraseBefore, keyboardFor, pressEnter, typeInto, wantsKeyboard } from "@/components/osk/type-into";
import { PinInput } from "@/components/pin-pad";
import { Dialog } from "@/components/ui/dialog";
import { setLanguage } from "@/lib/i18n";

function Form({ onSubmit }: { onSubmit: (value: string) => void }) {
  const [name, setName] = useState("");
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit(name);
      }}
    >
      <label>
        Nom
        <input value={name} onChange={(e) => setName(e.target.value)} />
      </label>
      <label>
        Prix
        <input inputMode="decimal" defaultValue="" />
      </label>
      <p data-testid="echo">{name}</p>
    </form>
  );
}

const press = (label: string) => fireEvent.click(screen.getByRole("button", { name: label }));

describe("on-screen keyboard", () => {
  beforeEach(() => {
    document.documentElement.dataset.osk = "1";
  });
  afterEach(() => {
    delete document.documentElement.dataset.osk;
  });

  it("types into a React field, erases, shifts and submits", () => {
    let submitted = "";
    render(
      <>
        <Form onSubmit={(v) => (submitted = v)} />
        <OnScreenKeyboard />
      </>,
    );
    act(() => screen.getByLabelText("Nom").focus());
    expect(screen.getByRole("group", { name: "Clavier à l'écran" })).toBeInTheDocument();
    press("k");
    press("a");
    press("r");
    press("Effacer");
    press("Maj");
    press("R");
    expect(screen.getByTestId("echo")).toHaveTextContent("kaR");
    press("Entrée");
    expect(submitted).toBe("kaR");
  });

  it("gives a number field a keypad: digits and the decimal comma (finding 19)", () => {
    function Quantity() {
      const [qty, setQty] = useState("");
      const [pin, setPin] = useState("");
      return (
        <>
          <label>
            Quantité
            <NumericInput kind="qty" value={qty} onValueChange={setQty} />
          </label>
          <label>
            PIN
            <PinInput value={pin} onChange={setPin} />
          </label>
        </>
      );
    }
    render(
      <>
        <Quantity />
        <OnScreenKeyboard />
      </>,
    );
    act(() => screen.getByLabelText("Quantité").focus());
    const panel = screen.getByRole("group", { name: "Clavier à l'écran" });
    expect(panel).toHaveAttribute("data-osk-kind", "decimal");
    expect(screen.queryByRole("button", { name: "a" })).not.toBeInTheDocument();
    for (const key of ["1", "2", ",", "5"]) press(key);
    expect(screen.getByLabelText("Quantité")).toHaveValue("12,5");
    press("Effacer");
    expect(screen.getByLabelText("Quantité")).toHaveValue("12,");
    // A PIN gets digits only.
    act(() => screen.getByLabelText("PIN").focus());
    expect(screen.queryByRole("button", { name: "," })).not.toBeInTheDocument();
    for (const key of ["2", "5", "8", "0"]) press(key);
    expect(screen.getByLabelText("PIN")).toHaveValue("2580");
    press("Masquer le clavier");
    expect(screen.queryByRole("group", { name: "Clavier à l'écran" })).not.toBeInTheDocument();
  });

  it("publishes its height while it is open, so dialogs and screens stay above it (finding 20)", () => {
    render(
      <>
        <Form onSubmit={() => undefined} />
        <OnScreenKeyboard />
      </>,
    );
    expect(document.documentElement.style.getPropertyValue("--osk-inset")).toBe("");
    act(() => screen.getByLabelText("Nom").focus());
    expect(document.documentElement.style.getPropertyValue("--osk-inset")).toMatch(/^\d+px$/);
    press("Masquer le clavier");
    expect(document.documentElement.style.getPropertyValue("--osk-inset")).toBe("");
  });

  it("speaks Arabic in Arabic", async () => {
    await setLanguage("ar");
    render(
      <>
        <Form onSubmit={() => undefined} />
        <OnScreenKeyboard />
      </>,
    );
    act(() => screen.getByLabelText("Nom").focus());
    press("ب");
    expect(screen.getByTestId("echo")).toHaveTextContent("ب");
    press("AZERTY");
    expect(screen.getByRole("button", { name: "q" })).toBeInTheDocument();
  });

  it("typing inside a dialog keeps the dialog open", () => {
    render(
      <>
        <Dialog open onOpenChange={() => undefined} title="Client">
          <Form onSubmit={() => undefined} />
        </Dialog>
        <OnScreenKeyboard />
      </>,
    );
    act(() => screen.getByLabelText("Nom").focus());
    fireEvent.pointerDown(screen.getByRole("button", { name: "z" }));
    press("z");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByTestId("echo")).toHaveTextContent("z");
  });

  it("a key is not a press outside the dialog, a press on the page still is", async () => {
    const onOpenChange = vi.fn();
    render(
      <>
        <p>Page</p>
        <Dialog open onOpenChange={onOpenChange} title="Client">
          <Form onSubmit={() => undefined} />
        </Dialog>
        <OnScreenKeyboard />
      </>,
    );
    act(() => screen.getByLabelText("Nom").focus());
    // Radix listens for presses outside one tick after the dialog opens, and closes on the click.
    const tick = () => act(() => new Promise((resolve) => setTimeout(resolve, 10)));
    await tick();
    const key = screen.getByRole("button", { name: "z" });
    fireEvent.pointerDown(key);
    fireEvent.click(key);
    await tick();
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(screen.getByTestId("echo")).toHaveTextContent("z");
    expect(screen.getByLabelText("Nom")).toHaveFocus();
    fireEvent.pointerDown(screen.getByText("Page"));
    fireEvent.click(screen.getByText("Page"));
    await tick();
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("does nothing when this PC did not ask for it", () => {
    delete document.documentElement.dataset.osk;
    render(
      <>
        <Form onSubmit={() => undefined} />
        <OnScreenKeyboard />
      </>,
    );
    act(() => screen.getByLabelText("Nom").focus());
    expect(screen.queryByRole("group")).not.toBeInTheDocument();
  });

  it("comes up for the field that has the focus when the choice is switched on, and goes when it is off", async () => {
    delete document.documentElement.dataset.osk;
    render(
      <>
        <Form onSubmit={() => undefined} />
        <OnScreenKeyboard />
      </>,
    );
    act(() => screen.getByLabelText("Nom").focus());
    act(() => {
      document.documentElement.dataset.osk = "1";
    });
    expect(await screen.findByRole("group", { name: "Clavier à l'écran" })).toBeInTheDocument();
    act(() => {
      document.documentElement.dataset.osk = "0";
    });
    await vi.waitFor(() => expect(screen.queryByRole("group")).not.toBeInTheDocument());
  });

  it("stays visible to assistive tech when a dialog opens on a field of its own", async () => {
    function View({ open }: { open: boolean }) {
      return (
        <>
          <label>
            Recherche
            <input />
          </label>
          <Dialog open={open} onOpenChange={() => undefined} title="Client">
            <label>
              Client
              <input autoFocus />
            </label>
          </Dialog>
          <OnScreenKeyboard />
        </>
      );
    }
    const { rerender } = render(<View open={false} />);
    act(() => screen.getByLabelText("Recherche").focus());
    rerender(<View open />);
    await act(() => new Promise((resolve) => setTimeout(resolve, 10)));
    expect(screen.getByRole("textbox", { name: "Client" })).toHaveFocus();
    // A modal hides the rest of the page (`aria-hidden`); a panel opened for the dialog's field
    // is a new node, outside what it hid.
    expect(screen.getByRole("group", { name: "Clavier à l'écran" })).toBeInTheDocument();
  });
});

describe("typing helpers", () => {
  const field = (attrs: Record<string, string> = {}, value = "") => {
    const input = document.createElement("input");
    for (const [name, v] of Object.entries(attrs)) input.setAttribute(name, v);
    input.value = value;
    document.body.append(input);
    return input;
  };
  afterEach(() => document.body.replaceChildren());

  it("types at the caret, over the selection, within maxlength, and erases whole characters", () => {
    const input = field({}, "abcd");
    input.setSelectionRange(1, 3);
    typeInto(input, "X");
    expect([input.value, input.selectionStart]).toEqual(["aXd", 2]);
    const full = field({ maxlength: "4" }, "abcd");
    const typed = vi.fn();
    full.addEventListener("input", typed);
    typeInto(full, "e");
    expect([full.value, typed.mock.calls.length]).toEqual(["abcd", 0]);
    const room = field({ maxlength: "5" }, "abc");
    typeInto(room, "لا!");
    expect(room.value).toBe("abcلا");
    const emoji = field({}, "a😀");
    eraseBefore(emoji);
    expect(emoji.value).toBe("a");
  });

  it("types at the end of an email field, which has no caret", () => {
    const email = field({ type: "email" }, "ali");
    expect(() => typeInto(email, "@")).not.toThrow();
    expect(email.value).toBe("ali@");
    eraseBefore(email);
    expect(email.value).toBe("ali");
  });

  it("Enter: the field's own handler first, never through a disabled save button, a new line in a text area", () => {
    const submitted = vi.fn((event: Event) => event.preventDefault());
    const form = document.createElement("form");
    form.addEventListener("submit", submitted);
    const input = document.createElement("input");
    const save = document.createElement("button");
    form.append(input, save);
    document.body.append(form);

    input.addEventListener("keydown", (event) => event.preventDefault(), { once: true });
    pressEnter(input);
    expect(submitted).not.toHaveBeenCalled();
    save.disabled = true;
    pressEnter(input);
    expect(submitted).not.toHaveBeenCalled();
    save.disabled = false;
    pressEnter(input);
    expect(submitted).toHaveBeenCalledTimes(1);

    const notes = document.createElement("textarea");
    notes.value = "ligne";
    document.body.append(notes);
    pressEnter(notes);
    expect(notes.value).toBe("ligne\n");
  });

  it("serves text, number and PIN fields with the keys they need", () => {
    expect(wantsKeyboard(field())).toBe(true);
    expect(wantsKeyboard(field({ type: "tel" }))).toBe(true);
    expect(wantsKeyboard(document.createElement("textarea"))).toBe(true);
    expect(keyboardFor(field({ type: "password" }))).toBe("text");
    expect(keyboardFor(field({ type: "password", inputmode: "numeric" }))).toBe("numeric");
    expect(keyboardFor(field({ inputmode: "numeric" }))).toBe("numeric");
    expect(keyboardFor(field({ inputmode: "decimal" }))).toBe("decimal");
    expect(wantsKeyboard(field({ type: "checkbox" }))).toBe(false);
    expect(wantsKeyboard(field({ "data-osk": "off" }))).toBe(false);
    expect(wantsKeyboard(field({ readonly: "" }))).toBe(false);
    expect(wantsKeyboard(field({ disabled: "" }))).toBe(false);
    expect(wantsKeyboard(document.createElement("button"))).toBe(false);
  });
});
