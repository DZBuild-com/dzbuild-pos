import { useEffect, useState } from "react";
import type { DisplayPrefs } from "@/lib/ipc/types";

/** `pos_core::local::TextSize::percent` (UX-009 font scaling): every size is in rem. */
const PERCENT = { normal: 100, large: 115, larger: 130 } as const;

/** This PC's display choices on `<html>`: `data-light` (light.css), `data-osk` (the on-screen
 * keyboard) and the root font size. */
export function applyDisplay(prefs: DisplayPrefs): void {
  const root = document.documentElement;
  root.dataset.light = prefs.lightMode ? "1" : "0";
  root.dataset.osk = prefs.onscreenKeyboard ? "1" : "0";
  root.style.fontSize = `${PERCENT[prefs.textSize]}%`;
}

const isLight = () => document.documentElement.dataset.light === "1";

/** True while light mode is on (the till grid skips product images). */
export function useLightMode(): boolean {
  const [light, setLight] = useState(isLight);
  useEffect(() => {
    const observer = new MutationObserver(() => setLight(isLight()));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-light"] });
    return () => observer.disconnect();
  }, []);
  return light;
}
