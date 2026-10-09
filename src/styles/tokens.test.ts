import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(path.resolve(import.meta.dirname, "tokens.css"), "utf8");

const LIGHT: Record<string, string> = {
  "--bg": "#FBFAF7",
  "--surface": "#FFFFFF",
  "--surface-2": "#F3F1EB",
  "--line": "#E4E0D6",
  "--ink": "#1C1917",
  "--ink-2": "#5C5852",
  "--brand": "#FFC300",
  "--brand-ink": "#1C1917",
  "--brand-soft": "#FFF3C4",
  "--ok": "#1B7F3B",
  "--ok-soft": "#DDF3E3",
  "--warn": "#9A5B00",
  "--warn-soft": "#FFE7B8",
  "--danger": "#B42318",
  "--danger-soft": "#FBE3E0",
  "--info": "#1D4ED8",
  "--info-soft": "#DCE6FB",
  "--focus": "#1D4ED8",
  "--chart-1": "#2A78D6",
};

const DARK: Record<string, string> = {
  "--bg": "#161412",
  "--surface": "#1F1C19",
  "--surface-2": "#2A2622",
  "--line": "#3A3530",
  "--ink": "#F5F2EC",
  "--ink-2": "#B8B2A8",
  "--brand": "#FFC300",
  "--brand-ink": "#1C1917",
  "--brand-soft": "#3D3110",
  "--ok": "#4CC26F",
  "--ok-soft": "#12331C",
  "--warn": "#F0B34A",
  "--warn-soft": "#3A2A0A",
  "--danger": "#F0716A",
  "--danger-soft": "#3B1512",
  "--info": "#7FA6FF",
  "--info-soft": "#14213F",
  "--focus": "#7FA6FF",
  "--chart-1": "#3987E5",
};

function block(selector: string): Record<string, string> {
  const start = css.indexOf(`${selector} {`);
  if (start < 0) throw new Error(`missing block ${selector}`);
  const body = css.slice(start, css.indexOf("}", start));
  return Object.fromEntries(
    [...body.matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g)].map((m) => [m[1]!, m[2]!.trim().toUpperCase()]),
  );
}

describe("design tokens (spec §10.1)", () => {
  it("defines every light value", () => {
    expect(block(":root")).toEqual(LIGHT);
  });

  it("defines every dark value on data-theme=dark", () => {
    expect(block(':root[data-theme="dark"]')).toEqual(DARK);
  });

  it("follows the system dark mode unless the user forced light", () => {
    expect(css).toContain("@media (prefers-color-scheme: dark)");
    expect(block(':root:not([data-theme="light"])')).toEqual(DARK);
  });
});
