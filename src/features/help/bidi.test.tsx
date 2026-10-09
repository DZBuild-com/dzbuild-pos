import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HelpMarkdown } from "@/features/help/markdown";

/**
 * Finding 24: in an Arabic sentence a Latin file name keeps its shape only inside a bidi
 * isolate, else ".dzpos" shows as "dzpos." and "(.dzpos)" as "(dzpos.)". The Arabic catalogues
 * write them between U+2066 and U+2069 (as \u escapes), and the help renders them in
 * `<bdi dir="ltr">`.
 */
const LOOSE_NAME = /(^|[( ])\.[A-Za-z]/u;

const catalogues = import.meta.glob<Record<string, unknown>>("../../locales/ar/*.json", {
  import: "default",
  eager: true,
});
const helpPages = import.meta.glob<string>("../../help/ar/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
});

function strings(value: unknown, at: string, out: Array<[string, string]>) {
  if (typeof value === "string") out.push([at, value]);
  else if (value && typeof value === "object") {
    for (const [key, inner] of Object.entries(value)) strings(inner, at ? `${at}.${key}` : key, out);
  }
  return out;
}

/** The text of `root` that no element with a `dir` (an isolate) holds. */
function looseText(root: Node, out: string[] = []): string[] {
  for (const child of Array.from(root.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) out.push(child.textContent ?? "");
    else if (child instanceof Element && !child.hasAttribute("dir")) looseText(child, out);
  }
  return out;
}

describe("Latin file names in Arabic", () => {
  it("are isolated in every Arabic catalogue string", () => {
    const found = Object.entries(catalogues).flatMap(([file, catalogue]) =>
      strings(catalogue, "", [])
        .filter(([, text]) => LOOSE_NAME.test(text))
        .map(([key]) => `${file}: ${key}`),
    );
    expect(found).toEqual([]);
    // The rule finds what it is for.
    expect(LOOSE_NAME.test("تصدير نسخة احتياطية (.dzpos)")).toBe(true);
    expect(LOOSE_NAME.test("تصدير نسخة احتياطية (\u2066.dzpos\u2069)")).toBe(false);
  });

  it("are isolated on every Arabic help page as rendered", () => {
    const found: string[] = [];
    for (const [file, text] of Object.entries(helpPages)) {
      const { container, unmount } = render(
        <HelpMarkdown text={text} checklistKey="bidi-test" onTopic={() => {}} />,
      );
      for (const piece of looseText(container)) {
        if (LOOSE_NAME.test(piece)) found.push(`${file}: ${piece}`);
      }
      unmount();
    }
    expect(found).toEqual([]);
    const { container } = render(
      <HelpMarkdown text={"# T\n\nملف .dzpos (.zip)"} checklistKey="bidi-test" onTopic={() => {}} />,
    );
    expect(Array.from(container.querySelectorAll("bdi[dir=ltr]")).map((b) => b.textContent)).toEqual([
      ".dzpos",
      ".zip",
    ]);
  });
});
