import { describe, expect, it } from "vitest";
import { parseHelp } from "@/features/help/markdown";

describe("help markdown subset", () => {
  it("reads the subset and refuses the rest", () => {
    const ok = parseHelp(
      "# Titre\n\nUn **mot** et `F2`.\n\n## Section\n\n- un\n- [ ] deux\n1. trois\n\n> astuce [aide](help:pos) [écran](route:/cash)\n",
    );
    expect(ok.problems).toEqual([]);
    expect(ok.blocks.map((b) => b.kind)).toEqual([
      "title",
      "paragraph",
      "section",
      "list",
      "checklist",
      "ordered",
      "tip",
    ]);
    const bad = parseHelp("# T\n### sous\n<b>x</b>\n| a | b |\n![img](x.png)\n[site](https://x.dz)\n");
    expect(bad.problems).toHaveLength(5);
    expect(parseHelp("Pas de titre").problems).toEqual(["line 1: the page must start with '# '"]);
  });
});
