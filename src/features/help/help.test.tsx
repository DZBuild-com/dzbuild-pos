import { fireEvent, screen } from "@testing-library/react";
import { Route, Routes, useLocation } from "react-router";
import { describe, expect, it } from "vitest";
import { NAV_ITEMS } from "@/app/nav";
import { HelpHotkey } from "@/features/help/HelpHotkey";
import { HelpPage } from "@/features/help/HelpPage";
import { parseHelp } from "@/features/help/markdown";
import { HELP_TOPICS, helpTopicFor } from "@/features/help/topics";
import { SETTINGS_SECTIONS } from "@/features/settings/sections";
import { LANGS, setLanguage } from "@/lib/i18n";
import { mockCommands, renderWithProviders } from "@/test/render";

const pages = import.meta.glob<string>("../../help/*/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
});
const page = (lang: string, topic: string) => pages[`../../help/${lang}/${topic}.md`];
const count = (text: string, prefix: string) => text.split("\n").filter((l) => l.startsWith(prefix)).length;

describe("help centre", () => {
  it("every route and settings section has a help page in ar, fr and en with the same sections", () => {
    const routes = [
      // "/settings" itself only redirects to its first section; every section is listed below
      ...NAV_ITEMS.map((item) => item.path).filter((path) => path !== "/settings"),
      "/pos",
      "/help",
      ...SETTINGS_SECTIONS.map((section) => `/settings/${section.path}`),
    ];
    for (const route of routes) {
      const topic = helpTopicFor(route);
      expect(
        HELP_TOPICS.some((t) => t.id === topic),
        `${route} → ${topic}`,
      ).toBe(true);
      expect(topic === "first-day" && route !== "/help", `${route} falls back to first-day`).toBe(false);
    }
    for (const { id } of HELP_TOPICS) {
      const texts = LANGS.map((lang) => page(lang, id));
      texts.forEach((text, i) => expect(text, `${LANGS[i]}/${id}.md`).toBeTruthy());
      const [ar, fr, en] = texts as [string, string, string];
      for (const text of [ar, fr, en]) expect(parseHelp(text).problems, id).toEqual([]);
      expect(count(ar, "## "), `${id}: ar sections`).toBe(count(fr, "## "));
      expect(count(en, "## "), `${id}: en sections`).toBe(count(fr, "## "));
      expect(count(ar, "- [ ]"), `${id}: ar checklist`).toBe(count(fr, "- [ ]"));
      expect(count(en, "- [ ]"), `${id}: en checklist`).toBe(count(fr, "- [ ]"));
    }
    expect(Object.keys(pages)).toHaveLength(HELP_TOPICS.length * 3);
  });

  it("a pack page of the products or stock screen has its own page (M8)", () => {
    expect(helpTopicFor("/products", "?tab=chifa")).toBe("pharmacy");
    expect(helpTopicFor("/products", "?tab=register")).toBe("pharmacy");
    expect(helpTopicFor("/stock", "?tab=waste")).toBe("butcher");
    expect(helpTopicFor("/products", "?tab=departments")).toBe("catalogue-modules");
    expect(helpTopicFor("/products", "?tab=catalogue")).toBe("products");
    expect(helpTopicFor("/stock", "?tab=expiry")).toBe("stock");
  });

  it("F1 opens the page of the current screen in the UI language", async () => {
    mockCommands({
      support_contact: () => ({ whatsapp: null, url: "https://dzbuild.com/pos/support", label: null }),
    });
    await setLanguage("ar");
    let path = "";
    function Where() {
      path = useLocation().pathname;
      return null;
    }
    renderWithProviders(
      <>
        <HelpHotkey />
        <Where />
        <Routes>
          <Route
            path="/help/:topic"
            element={<HelpPage topic="settings-backups" onTopic={() => undefined} />}
          />
          <Route path="*" element={null} />
        </Routes>
      </>,
      { route: "/settings/backups" },
    );
    fireEvent.keyDown(window, { key: "F1" });
    expect(path).toBe("/help/settings-backups");
    expect(await screen.findByRole("heading", { level: 1, name: "النسخ الاحتياطية" })).toBeInTheDocument();
  });

  it("the first-day checklist remembers what was ticked", async () => {
    mockCommands({ support_contact: () => ({ whatsapp: null, url: null, label: null }) });
    const { unmount } = renderWithProviders(<HelpPage topic="first-day" onTopic={() => undefined} />);
    const first = (await screen.findAllByRole("checkbox"))[0]!;
    fireEvent.click(first);
    expect(first).toBeChecked();
    unmount();
    renderWithProviders(<HelpPage topic="first-day" onTopic={() => undefined} />);
    expect((await screen.findAllByRole("checkbox"))[0]).toBeChecked();
  });

  it("the quick card prints the page", async () => {
    mockCommands({ support_contact: () => ({ whatsapp: null, url: null, label: null }) });
    let printed = 0;
    window.print = () => {
      printed += 1;
    };
    renderWithProviders(<HelpPage topic="quick-card" onTopic={() => undefined} />);
    fireEvent.click(await screen.findByRole("button", { name: "Imprimer la fiche pratique" }));
    expect(printed).toBe(1);
  });
});
