import { ArrowUp } from "lucide-react";
import { useEffect, useLayoutEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation } from "react-router";
import { Button } from "@/components/ui/button";

/** The shell's scrolling body (`<main id="main">` in `AppShell`). */
export const MAIN_ID = "main";

/** Marks a nested scroll pane that a page change brings back to its top too. */
export const SCROLL_RESET_ATTR = "data-scroll-reset";

/** The till screens (`/pos`, `/pos/sale`…) have no scrolling body: no button there. */
const isTill = (pathname: string) => pathname === "/pos" || pathname.startsWith("/pos/");

/**
 * Every page change starts at the top of the page: `#main` and every `[data-scroll-reset]` pane
 * go back to 0 at once (no animation). Only the path counts: a filter or a tab in the search
 * (`?tab=`) keeps the scroll where it is.
 */
export function ScrollToTop() {
  const { pathname } = useLocation();
  // Keyed on the path alone: runs on every page change, never on a search-only one.
  useLayoutEffect(() => {
    const main = document.getElementById(MAIN_ID);
    if (main) main.scrollTop = 0;
    for (const pane of document.querySelectorAll<HTMLElement>(`[${SCROLL_RESET_ATTR}]`)) {
      pane.scrollTop = 0;
    }
  }, [pathname]);
  return null;
}

/** No smooth scroll in light mode (slow PC) or when the system asks for less motion. */
function scrollBehavior(): ScrollBehavior {
  const light = document.documentElement.dataset.light === "1";
  const reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  return light || reduced ? "auto" : "smooth";
}

/**
 * The shell's scroll on navigation: back to the top on every page change, and the floating
 * "back to top" button. The button is remounted per page (`key`): a new page starts hidden.
 */
export function MainScroll() {
  const { pathname } = useLocation();
  return (
    <>
      <ScrollToTop />
      {isTill(pathname) ? null : <BackToTop key={pathname} />}
    </>
  );
}

/**
 * A floating "back to top" button at the bottom end corner, shown once `#main` is scrolled more
 * than one window height. One passive scroll listener, read at most once a frame.
 */
function BackToTop() {
  const { t } = useTranslation();
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const main = document.getElementById(MAIN_ID);
    if (!main) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      setVisible(main.scrollTop > window.innerHeight);
    };
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    main.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      main.removeEventListener("scroll", onScroll);
      if (frame) cancelAnimationFrame(frame);
    };
  }, []);

  if (!visible) return null;

  const toTop = () => {
    const main = document.getElementById(MAIN_ID);
    if (!main) return;
    if (typeof main.scrollTo === "function") main.scrollTo({ top: 0, behavior: scrollBehavior() });
    else main.scrollTop = 0;
    // The button leaves at the top: the focus goes to the page, not to nowhere.
    main.focus({ preventScroll: true });
  };

  return (
    <Button
      variant="outline"
      size="icon"
      aria-label={t("common:backToTop")}
      title={t("common:backToTop")}
      onClick={toTop}
      className="fixed end-4 bottom-[calc(1rem+var(--osk-inset,0px))] z-40 rounded-full shadow-overlay"
    >
      <ArrowUp aria-hidden />
    </Button>
  );
}
