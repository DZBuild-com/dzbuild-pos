import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { i18n, isLang, isLanguageLoaded, type Lang, loadLanguage, setLanguage } from "@/lib/i18n";

/** Current UI language; components re-render when it changes. */
export function useLang(): Lang {
  const { i18n: instance } = useTranslation();
  return isLang(instance.language) ? instance.language : "fr";
}

/** Applies a language coming from data (login-screen setting, session) when it changes. */
export function useApplyLanguage(lang: Lang | undefined): void {
  useEffect(() => {
    if (lang && i18n.language !== lang) void setLanguage(lang);
  }, [lang]);
}

/** True once the texts of every language in `langs` (a module constant) are loaded: a screen
 * that shows several languages at once, whatever the UI's. */
export function useLanguagesLoaded(langs: readonly Lang[]): boolean {
  const [loaded, setLoaded] = useState(() => langs.every(isLanguageLoaded));
  useEffect(() => {
    let live = true;
    void Promise.all(langs.map(loadLanguage)).then(() => {
      if (live) setLoaded(true);
    });
    return () => {
      live = false;
    };
  }, [langs]);
  return loaded;
}
