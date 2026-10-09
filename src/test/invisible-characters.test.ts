import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const SRC = path.resolve(import.meta.dirname, "..");

/** Hand-written code: every .ts/.tsx/.css under src/ except the generated bindings. */
function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = path.join(dir, name);
    if (statSync(full).isDirectory()) return sourceFiles(full);
    return /\.(ts|tsx|css)$/.test(name) && name !== "bindings.ts" ? [full] : [];
  });
}

// Characters a reviewer cannot see in the source: no-break and other spaces, soft hyphen, zero-width
// and joiner characters, bidi marks, embeddings, overrides and isolates (U+2066-U+2069), the BOM.
// Visible punctuation between these ranges ("…" U+2026, dashes) is allowed.
// The combining grapheme joiner (U+034F) is matched outside the class: inside it, it would combine
// with its neighbour (ESLint no-misleading-character-class).
const INVISIBLE = /\u034F|[\u00A0\u00AD\u061C\u180E\u2000-\u200F\u2028-\u202F\u205F-\u206F\u3000\uFEFF]/gu;

describe("source files", () => {
  const files = sourceFiles(SRC);

  it("are found", () => {
    // Proves the scan reads real sources, including the two it guards first.
    const names = files.map((file) => path.relative(SRC, file).replaceAll("\\", "/"));
    expect(names).toContain("components/bar-chart.tsx");
    expect(names).toContain("components/data-table/cells.tsx");
  });

  it("write invisible characters as \\u escapes, never as raw characters", () => {
    const found: string[] = [];
    for (const file of files) {
      const lines = readFileSync(file, "utf8").split("\n");
      lines.forEach((line, i) => {
        for (const match of line.matchAll(INVISIBLE)) {
          const code = match[0].codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0");
          found.push(`${path.relative(SRC, file).replaceAll("\\", "/")}:${i + 1} U+${code}`);
        }
      });
    }
    expect(found).toEqual([]);
  });
});
