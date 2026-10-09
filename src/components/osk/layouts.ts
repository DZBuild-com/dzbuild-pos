export interface Key {
  key: string;
  shift?: string;
}

/** One key per character of `base`; Shift types the same place in `shifted`, or the capital (an
 * Arabic letter or a digit is its own capital). */
const row = (base: string, shifted?: string): Key[] => {
  const shifts = shifted === undefined ? undefined : Array.from(shifted);
  return Array.from(base).map((key, i) => ({ key, shift: shifts?.[i] ?? key.toUpperCase() }));
};

/** AZERTY (French Windows, digits first: the shop types numbers more than `&é"'(`) and Arabic 101
 * (digits shown Western, UX-014; `لا` is one key, as on the keyboard). */
export const LAYOUTS = {
  azerty: [
    [...row("1234567890", "&é\"'(-è_çà"), { key: "@", shift: "°" }],
    row("azertyuiop"),
    row("qsdfghjklmù"),
    [
      ...row("wxcvbn"),
      { key: "é", shift: "É" },
      { key: "è", shift: "È" },
      { key: "à", shift: "À" },
      { key: "ç", shift: "Ç" },
      { key: ",", shift: "?" },
      { key: ".", shift: ":" },
    ],
  ],
  arabic: [
    row("1234567890"),
    row("ضصثقفغعهخحجد"),
    row("شسيبلاتنمكط"),
    [...row("ئءؤر"), { key: "لا", shift: "لا" }, ...row("ىةوزظذ")],
  ],
} satisfies Record<string, Key[][]>;

export type LayoutName = keyof typeof LAYOUTS;
