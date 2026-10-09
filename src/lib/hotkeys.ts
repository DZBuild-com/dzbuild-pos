import { useEffect, useRef } from "react";

export type Hotkey =
  | "F1"
  | "F2"
  | "F3"
  | "F4"
  | "F5"
  | "F6"
  | "F7"
  | "F8"
  | "F9"
  | "F10"
  | "F11"
  | "F12"
  | "Escape"
  | `Ctrl+${0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9}`;

/** F-keys, Esc and Ctrl+digit for a screen (UX-017; the packs' till keys are Ctrl+1…Ctrl+9). A
 * dialog, when open, owns the keyboard. The digit is read from the physical key too, so an
 * AZERTY or Arabic layout (where the top row types `&é"'…` without Shift) gives the same key. */
export function useHotkeys(map: Partial<Record<Hotkey, () => void>>, enabled = true): void {
  const current = useRef(map);
  useEffect(() => {
    current.current = map;
  });
  useEffect(() => {
    if (!enabled) return undefined;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.altKey || event.metaKey) return;
      if (document.querySelector('[role="dialog"], [role="alertdialog"]')) return;
      const digit = /^Digit([0-9])$/.exec(event.code)?.[1] ?? (/^[0-9]$/.test(event.key) ? event.key : null);
      const key = event.ctrlKey ? (digit === null ? null : `Ctrl+${digit}`) : event.key;
      const action = key === null ? undefined : current.current[key as Hotkey];
      if (action) {
        event.preventDefault();
        action();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [enabled]);
}

/** A Ctrl shortcut of a screen (Ctrl+S saves a draft, Ctrl+D loads a quote at the till): read
 * while a field has the focus too, by the physical key so an AZERTY or Arabic layout types it
 * the same. A dialog, when open, owns the keyboard. */
export function useCtrlHotkey(letter: string, action: () => void, enabled = true): void {
  const current = useRef(action);
  useEffect(() => {
    current.current = action;
  });
  useEffect(() => {
    if (!enabled) return undefined;
    const code = `Key${letter.toUpperCase()}`;
    const onKeyDown = (event: KeyboardEvent) => {
      if (!event.ctrlKey || event.altKey || event.metaKey || event.shiftKey) return;
      if (event.code !== code && event.key.toLowerCase() !== letter.toLowerCase()) return;
      if (document.querySelector('[role="dialog"], [role="alertdialog"]')) return;
      event.preventDefault();
      current.current();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [enabled, letter]);
}
