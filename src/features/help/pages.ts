import type { Lang } from "@/lib/i18n";

// One lazy chunk per page: the help works offline and costs nothing until it is opened.
const files = import.meta.glob<string>("../../help/*/*.md", { query: "?raw", import: "default" });

/** The page of `topic` in `lang` (French when that language lacks it), or `null` for no such page. */
export async function loadHelpPage(lang: Lang, topic: string): Promise<string | null> {
  const load = files[`../../help/${lang}/${topic}.md`] ?? files[`../../help/fr/${topic}.md`];
  return load ? load() : null;
}

/** The `# ` line of a page. */
export function helpTitle(text: string | null): string | null {
  const first = text?.split("\n", 1)[0] ?? "";
  return first.startsWith("# ") ? first.slice(2).trim() : null;
}
