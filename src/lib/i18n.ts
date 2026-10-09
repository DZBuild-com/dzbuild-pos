import i18next from "i18next";
import { initReactI18next } from "react-i18next";

export const LANGS = ["ar", "fr", "en"] as const;
export type Lang = (typeof LANGS)[number];

/** The namespaces only back-office screens use: each language's are a chunk of their own
 * (`src/locales/bundles/<lang>-office.ts`), read when the first back-office screen opens. The
 * vertical packs' (M8) are there too, out of the till's texts: the setup wizard, the
 * pack screens and the till's pack extensions read that chunk when they open. */
export const BACK_OFFICE_NAMESPACES = [
  "products",
  "promotions",
  "stock",
  "documents",
  "purchases",
  "reports",
  "imports",
  "tables",
  "fiscal",
  // M8 packs (out of the till's texts): the activity templates and modules, the
  // restaurant's floor and editors, the pharmacy and the butcher; the setup wizard, the pack
  // screens and the till's pack extensions read this chunk when they open
  "packs",
  "restaurant",
  "pharmacy",
  "butcher",
  // M8 packs (out of the till's texts): the repair board and the flexy tab, the
  // instalment contracts, the customer orders and the appointment book, the customer prices and
  // the delivery rounds; the till's pack dialogs read this chunk when they load
  "phones",
  "services",
  "instalments",
  "orders",
  "wholesale",
  // M8 packs: their screens are back-office ones; a till dialog of a pack (the gift voucher sold
  // from the menu) loads these texts with its own chunk (`loadBackOffice`).
  "loyalty",
  "jewellery",
  "staff",
  "zakat",
  "privacy",
  // M8 (Task 30): the back office's own settings words (payments, till keys, regulatory packs,
  // duplication…), out of the till's `settings` texts (the /pos budget)
  "config",
  // The /pos budget: the settings screens, the audit journal's words and
  // the backups' are back-office texts; the fatal screen's guided restore reads them with its own
  // chunk, and the permission names a till's approval shows are `auth`'s.
  "settings",
  // M9: the licence's words (the activation screen, Settings › Licence, the read-only banner and
  // the till's card) load with their own chunks, only on a PC that needs them
  "license",
] as const;

/** The till's namespaces: the sign-in, the setup, the sale screen and its dialogs, the shell. */
export const TILL_NAMESPACES = [
  "common",
  "nav",
  "auth",
  "setup",
  "errors",
  "validation",
  "pos",
  "payments",
  "cash",
  "hardware",
  "customers",
  "billing",
  "labels",
  "sync",
  "help",
] as const;

export const NAMESPACES = [...TILL_NAMESPACES, ...BACK_OFFICE_NAMESPACES] as const;

type Catalogue = Record<string, unknown>;
type Bundle = Record<string, Catalogue>;

/** Every language as two chunks inside the app (offline): the till's texts, read before the
 * first paint in that language, and the back office's, read when a back-office screen first
 * opens. The till's JavaScript, the entry plus one language's till texts, stays under 300 kB
 * gzipped in each of the three languages (NFR-010, finding 28). */
const TILL: Record<Lang, () => Promise<Bundle>> = {
  ar: () => import("@/locales/bundles/ar-till").then((m) => m.default),
  fr: () => import("@/locales/bundles/fr-till").then((m) => m.default),
  en: () => import("@/locales/bundles/en-till").then((m) => m.default),
};
const BACK_OFFICE: Record<Lang, () => Promise<Bundle>> = {
  ar: () => import("@/locales/bundles/ar-office").then((m) => m.default),
  fr: () => import("@/locales/bundles/fr-office").then((m) => m.default),
  en: () => import("@/locales/bundles/en-office").then((m) => m.default),
};

export function isLang(value: unknown): value is Lang {
  return typeof value === "string" && (LANGS as readonly string[]).includes(value);
}

export const i18n = i18next.createInstance();

/** `lang` and `dir` on <html> follow the UI language at runtime (spec §11). */
export function applyDocumentLanguage(lang: Lang): void {
  const root = document.documentElement;
  root.lang = lang;
  root.dir = i18n.dir(lang);
}

void i18n.use(initReactI18next).init({
  resources: {},
  lng: "fr",
  fallbackLng: "fr",
  supportedLngs: [...LANGS],
  ns: [...NAMESPACES],
  defaultNS: "common",
  interpolation: { escapeValue: false },
  initAsync: false,
});

i18n.on("languageChanged", (lng) => {
  if (isLang(lng)) applyDocumentLanguage(lng);
});
applyDocumentLanguage("fr");

export function currentLang(): Lang {
  return isLang(i18n.language) ? i18n.language : "fr";
}

const loading = new Map<string, Promise<void>>();

/** The language asked for last: a switch whose texts arrive after a later one does not apply. */
let wanted: Lang = "fr";

/** A back-office screen opened once: every language switch from then on reads both chunks. */
let backOfficeWanted = false;

function load(set: "till" | "office", lang: Lang): Promise<void> {
  const key = `${set}:${lang}`;
  let pending = loading.get(key);
  if (!pending) {
    pending = (set === "till" ? TILL : BACK_OFFICE)[lang]().then((files) => {
      for (const [file, texts] of Object.entries(files)) {
        // `../ar/pos.json` → the `pos` namespace
        const namespace = file.slice(file.lastIndexOf("/") + 1, -".json".length);
        i18n.addResourceBundle(lang, namespace, texts, true, true);
      }
    });
    loading.set(key, pending);
    pending.catch(() => loading.delete(key));
  }
  return pending;
}

/** True when `lang`'s till texts are in memory. */
export function isLanguageLoaded(lang: Lang): boolean {
  return i18n.hasResourceBundle(lang, "common");
}

/** Makes `lang`'s texts available: the till's, and the back office's once a back-office screen
 * opened; the same promise until it is done, and a failed read is tried again next time. */
export function loadLanguage(lang: Lang): Promise<void> {
  const till = load("till", lang);
  if (!backOfficeWanted) return till;
  const key = `both:${lang}`;
  let both = loading.get(key);
  if (!both) {
    both = Promise.all([till, load("office", lang)]).then(() => undefined);
    loading.set(key, both);
    both.catch(() => loading.delete(key));
  }
  return both;
}

/** A back-office screen is opening: its texts in the UI language first (`routes.tsx` waits for
 * them with the screen's own chunk), and in every language switched to afterwards. */
export function loadBackOffice(): Promise<void> {
  backOfficeWanted = true;
  return loadLanguage(wanted);
}

/** The UI language of this PC's last session, for the first paint of the next start (read
 * before any database answers); a per-PC convenience in the browser storage. */
const UI_LANG_KEY = "dzpos.lang";

export function rememberedUiLang(): Lang | null {
  try {
    const value = window.localStorage.getItem(UI_LANG_KEY);
    return isLang(value) ? value : null;
  } catch {
    return null;
  }
}

function rememberUiLang(lang: Lang): void {
  try {
    window.localStorage.setItem(UI_LANG_KEY, lang);
  } catch {
    // a blocked storage only costs a language switch at the next start
  }
}

/** Loads the language's texts, then switches the UI (and <html lang dir>) to it. */
export async function setLanguage(lang: Lang): Promise<void> {
  wanted = lang;
  await loadLanguage(lang);
  if (wanted === lang) {
    await i18n.changeLanguage(lang);
    rememberUiLang(lang);
  }
}
