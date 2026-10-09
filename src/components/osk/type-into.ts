export type Field = HTMLInputElement | HTMLTextAreaElement;

/** `<input>` types that keep a caret (an `email` field has none: `setSelectionRange` throws). */
const CARET_TYPES = ["text", "search", "url", "tel", "password"];

const hasCaret = (el: Field): boolean => el instanceof HTMLTextAreaElement || CARET_TYPES.includes(el.type);

/** Sets the value as the browser does when a person types, so React sees an `input` event. */
function setValue(el: Field, value: string, caret: number): void {
  const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el) as object, "value")?.set;
  setter?.call(el, value);
  if (hasCaret(el)) el.setSelectionRange(caret, caret);
  el.dispatchEvent(new Event("input", { bubbles: true }));
}

function selection(el: Field): { start: number; end: number } {
  const start = hasCaret(el) ? (el.selectionStart ?? el.value.length) : el.value.length;
  const end = hasCaret(el) ? (el.selectionEnd ?? el.value.length) : el.value.length;
  return { start, end };
}

/** Types `text` at the caret (over the selection), within the field's `maxlength` like typing. */
export function typeInto(el: Field, text: string): void {
  const { start, end } = selection(el);
  const kept = el.value.length - (end - start);
  const room = el.maxLength >= 0 ? Math.max(0, el.maxLength - kept) : Infinity;
  const typed = Array.from(text).slice(0, room).join("");
  if (typed === "" && start === end) return;
  setValue(el, el.value.slice(0, start) + typed + el.value.slice(end), start + typed.length);
}

/** Backspace: erases the selection, or the one character (not half a surrogate pair) before the caret. */
export function eraseBefore(el: Field): void {
  const { start, end } = selection(el);
  if (start !== end) {
    setValue(el, el.value.slice(0, start) + el.value.slice(end), start);
    return;
  }
  if (start === 0) return;
  const before = Array.from(el.value.slice(0, start)).slice(0, -1).join("");
  setValue(el, before + el.value.slice(start), before.length);
}

/** The browser's implicit submission: through the form's default (first) submit button, never
 * while that button is disabled (a save in flight), and without one when the form has none. */
function submitImplicitly(form: HTMLFormElement): void {
  const button = Array.from(form.elements).find(
    (e): e is HTMLButtonElement | HTMLInputElement =>
      (e instanceof HTMLButtonElement || e instanceof HTMLInputElement) &&
      (e.type === "submit" || e.type === "image"),
  );
  if (!button) form.requestSubmit();
  else if (!button.disabled) form.requestSubmit(button);
}

/** Enter: a `keydown` the field's own handlers see first; when none of them took it, a text field
 * submits its form and a text area gets a new line. */
export function pressEnter(el: Field): void {
  const event = new KeyboardEvent("keydown", {
    key: "Enter",
    code: "Enter",
    bubbles: true,
    cancelable: true,
  });
  if (!el.dispatchEvent(event)) return;
  if (el instanceof HTMLTextAreaElement) typeInto(el, "\n");
  else if (el.form) submitImplicitly(el.form);
}

const TEXT_TYPES = ["text", "search", "email", "tel", "url", "password"];

/** Which keys a field gets: letters, or a keypad for a number (`numeric`: digits only, a PIN or a
 * count; `decimal`: digits and the decimal comma, an amount, a quantity or a price). */
export type KeyboardKind = "text" | "numeric" | "decimal";

/** The keys the keyboard offers `target`, or `null` when it serves none: not a text field, read
 * only, disabled, or `data-osk="off"` (the scan field; a form with keys of its own, the payment
 * keypad). A password is a PIN when it says `inputmode="numeric"`, else a pass phrase. */
export function keyboardFor(target: EventTarget | null): KeyboardKind | null {
  if (!(target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement)) return null;
  if (target.readOnly || target.disabled || target.dataset.osk === "off") return null;
  const mode = target.getAttribute("inputmode") ?? "";
  if (mode === "decimal") return "decimal";
  if (mode === "numeric") return "numeric";
  return target instanceof HTMLTextAreaElement || TEXT_TYPES.includes(target.type) ? "text" : null;
}

/** A field the keyboard serves ([`keyboardFor`]). */
export function wantsKeyboard(target: EventTarget | null): target is Field {
  return keyboardFor(target) !== null;
}
