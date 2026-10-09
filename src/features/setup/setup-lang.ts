import { type Lang, isLang } from "@/lib/i18n";

/** This PC's language for the setup wizard, kept in the browser storage of this PC: a
 * convenience only (the shop's languages live in its database once it exists). */
const KEY = "dzpos.setup.lang";

/** The language chosen in the wizard on this PC before, if any (unreadable storage: none). */
export function storedSetupLang(): Lang | null {
  try {
    const value = window.localStorage.getItem(KEY);
    return isLang(value) ? value : null;
  } catch {
    return null;
  }
}

/** Keeps `lang` for the next time the wizard shows (leaving the demo, a restart). */
export function rememberSetupLang(lang: Lang): void {
  try {
    window.localStorage.setItem(KEY, lang);
  } catch {
    // a blocked storage only costs the owner a click next time
  }
}

/** The wizard's language: the one chosen on this PC before, else Windows' display language
 * when it is Arabic or English, else French. */
export function initialSetupLang(): Lang {
  const stored = storedSetupLang();
  if (stored) return stored;
  const system = typeof navigator === "undefined" ? "" : navigator.language.toLowerCase();
  if (system.startsWith("ar")) return "ar";
  if (system.startsWith("en")) return "en";
  return "fr";
}
