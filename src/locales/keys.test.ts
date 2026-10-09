import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import { i18n, LANGS, loadLanguage, NAMESPACES } from "@/lib/i18n";

const SRC = path.resolve(import.meta.dirname, "..");

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = path.join(dir, name);
    if (statSync(full).isDirectory()) return name === "locales" ? [] : sourceFiles(full);
    const isSource = /\.(ts|tsx)$/.test(name) && !/\.test\.tsx?$/.test(name) && name !== "bindings.ts";
    return isSource ? [full] : [];
  });
}

const KEY = new RegExp(`["'\`]((?:${NAMESPACES.join("|")}):[A-Za-z0-9_.]+)["'\`]`, "g");

describe("translation keys used in the code", () => {
  // Arabic and English load on first use (src/lib/i18n.ts).
  beforeAll(() => Promise.all(LANGS.map(loadLanguage)));
  const used = new Set<string>();
  for (const file of sourceFiles(SRC)) {
    for (const match of readFileSync(file, "utf8").matchAll(KEY)) used.add(match[1]!);
  }

  it("are found in the sources", () => {
    // App.tsx (later the Splash component) renders this key: proves the scan reads real sources.
    expect([...used]).toContain("common:status.loading");
  });

  for (const lang of LANGS) {
    it(`all exist in ${lang}`, () => {
      const missing = [...used].filter((key) => !i18n.exists(key, { lng: lang, count: 1 }));
      expect(missing).toEqual([]);
    });
  }
});
