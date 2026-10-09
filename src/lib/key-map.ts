import type { TFunction } from "i18next";
import { useMemo } from "react";
import { useTillKeyMap } from "@/lib/ipc/privacy";
import type { Hotkey } from "./hotkeys";

/**
 * UX-017: the till's actions and their default keys, in the order of the backend's
 * `till::DEFAULT_KEYS` (core Task 17; `key-map.test.ts` compares the two), as M1b, M2 and the
 * packs bind them. F11 stays M4's scale panel and is not remappable. The sale screen reads its
 * keys through `useKeyMap`, Settings › Raccourcis its presses through `hotkeyOf`: one rule.
 */
export const DEFAULT_KEYS = {
  help: "F1",
  search: "F2",
  customer: "F3",
  price_check: "F4",
  discount: "F5",
  hold: "F6",
  held: "F7",
  return: "F8",
  drawer: "F9",
  repeat_last: "F10",
  checkout: "F12",
  menu: "Escape",
  floor: "Ctrl+1",
  flexy: "Ctrl+2",
  gift_voucher: "Ctrl+3",
  serial: "Ctrl+4",
  department_keys: "Ctrl+5",
} as const satisfies Record<string, Hotkey>;

export type TillAction = keyof typeof DEFAULT_KEYS;

/** A key the till binds: F1–F12 but F11, Esc, Ctrl+0…Ctrl+9 (the backend's own rule). */
function isTillKey(key: string): key is Hotkey {
  return /^(F([1-9]|10|12)|Escape|Ctrl\+[0-9])$/.test(key);
}

/** The till's keys: the defaults overridden by `till.key_map` (UX-017); the defaults while the
 * map is not answered. */
export function useKeyMap(): Record<TillAction, Hotkey> {
  const { data } = useTillKeyMap();
  return useMemo(() => {
    const map: Record<TillAction, Hotkey> = { ...DEFAULT_KEYS };
    for (const [action, key] of data ?? []) {
      if (action in map && isTillKey(key)) map[action as TillAction] = key;
    }
    return map;
  }, [data]);
}

/** `aria-keyshortcuts` of a key: "F12", "Escape", "Control+5". */
export function ariaKey(key: Hotkey): string {
  return key.startsWith("Ctrl+") ? `Control+${key.slice(5)}` : key;
}

/** The keycap of a key in the UI language ("Échap", "Ctrl+5"). */
export function keyLabel(key: Hotkey, t: TFunction): string {
  return key.startsWith("Ctrl+")
    ? t("common:keys.ctrlDigit", { digit: key.slice(5) })
    : t(`common:keys.${key.toLowerCase()}`);
}

/** The key a press stands for at the till: F1–F12 but F11 (M4's scale panel), Ctrl + a digit
 * (the digit of the key's position, so AZERTY's Ctrl+& is Ctrl+1), or Escape; `null` for any
 * other press. The backend accepts the same keys (`key_map_invalid` otherwise). */
export function hotkeyOf(event: KeyboardEvent): string | null {
  if (event.altKey || event.metaKey) return null;
  if (event.ctrlKey) {
    if (event.shiftKey) return null;
    const digit = /^\d$/.test(event.key) ? event.key : /^(?:Digit|Numpad)(\d)$/.exec(event.code)?.[1];
    return digit === undefined ? null : `Ctrl+${digit}`;
  }
  if (event.shiftKey) return null;
  if (event.key === "Escape") return "Escape";
  return /^F([1-9]|1[02])$/.test(event.key) ? event.key : null;
}
