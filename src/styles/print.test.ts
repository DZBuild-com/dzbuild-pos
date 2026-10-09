import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

/** The paper of a printed help page (the quick card, `window.print()` in WebView2). */
const css = readFileSync(path.resolve(import.meta.dirname, "print.css"), "utf8").replace(
  /\/\*[\s\S]*?\*\//g,
  "",
);

/** The body of the `{ … }` block that `head` opens, nested blocks included. */
function body(source: string, head: RegExp): string {
  const match = head.exec(source);
  if (!match) throw new Error(`missing ${head}`);
  const open = match.index + match[0].length - 1;
  let depth = 0;
  for (let i = open; i < source.length; i += 1) {
    if (source[i] === "{") depth += 1;
    if (source[i] === "}" && --depth === 0) return source.slice(open + 1, i);
  }
  throw new Error(`unclosed ${head}`);
}

/** A flat block's declarations, `property → value`. */
function declarations(block: string): Record<string, string> {
  return Object.fromEntries(
    block
      .split(";")
      .map((line) => line.split(":").map((part) => part.trim()))
      .filter((pair): pair is [string, string] => pair.length === 2 && pair[0] !== ""),
  );
}

describe("a printed help page (the quick card)", () => {
  const print = body(css, /@media\s+print\s*\{/);

  it("asks for A4 with no page margin: Chromium prints its header and footer in that margin", () => {
    const page = declarations(body(print, /@page\s*\{/));
    expect(page.size).toBe("A4");
    expect(page.margin).toBe("0");
  });

  it("is the only @page rule, so nothing brings a margin back", () => {
    expect(css.match(/@page/g)).toHaveLength(1);
  });

  it("keeps the 12 mm on the sheet itself, on every page, across the full A4 width", () => {
    const sheet = declarations(body(print, /\.print-sheet\s*\{/));
    expect(sheet.padding).toBe("12mm");
    expect(sheet["box-sizing"]).toBe("border-box");
    expect(sheet["box-decoration-break"]).toBe("clone");
    expect(sheet["inline-size"]).toBe("100%");
    // the article's on-screen `max-w-3xl` would otherwise narrow the sheet on paper
    expect(sheet["max-inline-size"]).toBe("none");
  });
});
