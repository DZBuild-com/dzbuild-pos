import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

/** One row of `GLOSSARY.md`: its number and its concept (the English name of the business idea). */
interface Row {
  no: number;
  concept: string;
  cells: string[];
}

const GLOSSARY = path.resolve(import.meta.dirname, "GLOSSARY.md");

function rows(): Row[] {
  return readFileSync(GLOSSARY, "utf8")
    .split("\n")
    .filter((line) => /^\| \d+ \|/.test(line))
    .map((line) => {
      const cells = line
        .replace(/^\|/, "")
        .replace(/\|\s*$/, "")
        .split("|")
        .map((cell) => cell.trim());
      return { no: Number(cells[0]), concept: cells[1] ?? "", cells };
    });
}

// Spec §10.6: one concept, one word, one row. Every milestone appends its rows and the tasks cite
// them by number ("the Arabic follows glossary rows 101-114"), so a number means one concept only.
describe("the glossary", () => {
  const all = rows();

  it("is read", () => {
    // Proves the parser sees the real table: row 1 and the M3 rows appended after M2's row 88.
    expect(all[0]).toMatchObject({ no: 1, concept: "Consumer sale receipt" });
    expect(all.find((row) => row.concept === "Mandatory mentions")?.no).toBe(89);
    expect(all.find((row) => row.concept === "Auto-entrepreneur card")?.no).toBe(117);
  });

  it("numbers its rows 1, 2, 3… in order, each number once", () => {
    expect(all.map((row) => row.no)).toEqual(all.map((_, i) => i + 1));
  });

  it("names each concept once", () => {
    const seen = new Set<string>();
    const twice = all.filter((row) => (seen.has(row.concept) ? true : (seen.add(row.concept), false)));
    expect(twice.map((row) => `${row.no} ${row.concept}`)).toEqual([]);
  });

  it("gives every row its concept and its French, Arabic and English words", () => {
    const incomplete = all.filter(
      (row) => row.cells.length !== 6 || row.cells.slice(1, 5).some((cell) => !cell),
    );
    expect(incomplete.map((row) => `${row.no} ${row.concept}`)).toEqual([]);
  });
});
