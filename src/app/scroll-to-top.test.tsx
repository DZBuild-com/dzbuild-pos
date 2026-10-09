import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Link, MemoryRouter, Outlet, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";
import { MainScroll, SCROLL_RESET_ATTR } from "@/app/ScrollToTop";
import { SettingsLayout } from "@/features/settings/SettingsLayout";
import arCommon from "@/locales/ar/common.json";
import enCommon from "@/locales/en/common.json";
import frCommon from "@/locales/fr/common.json";
import { i18n, loadLanguage } from "@/lib/i18n";
import { ownerSession } from "@/test/fixtures";
import { missingKeys } from "@/test/render";

/** A shell like `AppShell`: the scroll helpers next to a scrolling `#main`. */
function Shell() {
  return (
    <>
      <MainScroll />
      <main id="main" tabIndex={-1}>
        <div data-testid="pane" {...{ [SCROLL_RESET_ATTR]: "" }} />
        <Link to="/b">page B</Link>
        <Link to="/b?tab=2">tab 2</Link>
        <Link to="/pos">till</Link>
        <Outlet />
      </main>
    </>
  );
}

function renderShell(route = "/a") {
  return render(
    <MemoryRouter initialEntries={[route]}>
      <Routes>
        <Route element={<Shell />}>
          <Route path="a" element={<p>A</p>} />
          <Route path="b" element={<p>B</p>} />
          <Route path="pos" element={<p>till</p>} />
        </Route>
      </Routes>
    </MemoryRouter>,
  );
}

const main = () => document.getElementById("main") as HTMLElement;

/** Scrolls `#main` and lets the frame-throttled listener read it. */
async function scrollMain(top: number) {
  main().scrollTop = top;
  fireEvent.scroll(main());
  await act(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => resolve(null)));
  });
}

describe("the shell's scroll on navigation", () => {
  beforeEach(() => {
    // jsdom has no layout: a 768 px window, `#main` at whatever `scrollTop` the test sets.
    Object.defineProperty(window, "innerHeight", { value: 768, configurable: true });
  });
  afterEach(() => vi.restoreAllMocks());

  it("a page change brings #main and the marked panes back to the top", async () => {
    renderShell();
    main().scrollTop = 900;
    screen.getByTestId("pane").scrollTop = 300;
    await userEvent.click(screen.getByRole("link", { name: "page B" }));
    expect(screen.getByText("B")).toBeInTheDocument();
    expect(main().scrollTop).toBe(0);
    expect(screen.getByTestId("pane").scrollTop).toBe(0);
  });

  it("a search-only change (a tab, a filter) keeps the scroll", async () => {
    renderShell("/b");
    main().scrollTop = 900;
    await userEvent.click(screen.getByRole("link", { name: "tab 2" }));
    expect(main().scrollTop).toBe(900);
  });

  it("the back-to-top button shows past one window height and scrolls to the top", async () => {
    const scrollTo = vi.fn();
    renderShell();
    main().scrollTo = scrollTo as unknown as typeof Element.prototype.scrollTo;
    const button = () => screen.queryByRole("button", { name: "Revenir en haut" });
    expect(button()).toBeNull();

    await scrollMain(500);
    expect(button()).toBeNull();

    await scrollMain(1200);
    expect(button()).toBeInTheDocument();
    expect(button()).toHaveClass("end-4");

    await userEvent.click(button() as HTMLElement);
    expect(scrollTo).toHaveBeenCalledWith(expect.objectContaining({ top: 0 }));
    expect(document.activeElement).toBe(main());

    await scrollMain(100);
    expect(button()).toBeNull();
  });

  it("the button is reachable with the keyboard", async () => {
    renderShell();
    const scrollTo = vi.fn();
    main().scrollTo = scrollTo as unknown as typeof Element.prototype.scrollTo;
    await scrollMain(1200);
    const button = screen.getByRole("button", { name: "Revenir en haut" });
    button.focus();
    await userEvent.keyboard("{Enter}");
    expect(scrollTo).toHaveBeenCalledTimes(1);
  });

  it("a page change hides the button; the till never shows it", async () => {
    renderShell();
    await scrollMain(1200);
    expect(screen.getByRole("button", { name: "Revenir en haut" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("link", { name: "till" }));
    await scrollMain(1200);
    expect(screen.queryByRole("button", { name: "Revenir en haut" })).toBeNull();
  });

  it("the button's label exists in Arabic, French and English", async () => {
    expect(arCommon.backToTop).toBe("العودة إلى الأعلى");
    expect(frCommon.backToTop).toBe("Revenir en haut");
    expect(enCommon.backToTop).toBe("Back to top");
    await loadLanguage("ar");
    await loadLanguage("en");
    missingKeys.clear();
    for (const lng of ["ar", "fr", "en"]) i18n.t("common:backToTop", { lng });
    expect([...missingKeys]).toEqual([]);
  });

  it("the settings menu, when it overflows, keeps the open section in view", async () => {
    const seen: string[] = [];
    vi.spyOn(HTMLElement.prototype, "scrollWidth", "get").mockReturnValue(2000);
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(400);
    const scrollIntoView = vi.fn(function (this: HTMLElement) {
      seen.push(this.getAttribute("href") ?? "");
    });
    const original = HTMLElement.prototype.scrollIntoView;
    HTMLElement.prototype.scrollIntoView = scrollIntoView;
    onTestFinished(() => {
      HTMLElement.prototype.scrollIntoView = original;
    });
    render(
      <MemoryRouter initialEntries={["/settings/shop"]}>
        <Routes>
          <Route path="settings" element={<SettingsLayout session={ownerSession} />}>
            <Route path="*" element={<p>section</p>} />
          </Route>
        </Routes>
      </MemoryRouter>,
    );
    const menu = screen.getByRole("navigation");
    const links = menu.querySelectorAll("a");
    const last = links[links.length - 1] as HTMLAnchorElement;
    await userEvent.click(last);
    expect(scrollIntoView).toHaveBeenLastCalledWith({ block: "nearest", inline: "nearest" });
    expect(seen.at(-1)).toBe(last.getAttribute("href"));
    expect(seen[0]).toBe("/settings/shop");
  });
});
