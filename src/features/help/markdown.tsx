import { type ReactNode, useId, useMemo, useState } from "react";
import { cn } from "@/lib/utils";
import { isHelpTopic } from "./topics";

/** A block of a help page: the subset `parseHelp` reads, nothing else. */
export type Block =
  | { kind: "title" | "section" | "paragraph" | "tip"; text: string }
  | { kind: "list" | "ordered" | "checklist"; items: string[] };

const CODE = /`[^`]*`/g;
const LINK = /\[([^\]]*)\]\(([^)]*)\)/g;
const ORDERED = /^\d+\. /;

/** Why a line is outside the help subset, or `null`. One problem per line. */
function lineProblem(line: string): string | null {
  if (line.startsWith("###")) return "only '# ' and '## ' headings are allowed";
  if (line.startsWith("<")) return "HTML is not allowed";
  if (line.startsWith("|")) return "tables are not allowed";
  if (line.includes("![")) return "images are not allowed";
  if ((line.match(/`/g)?.length ?? 0) % 2 !== 0) return "a `key` is not closed";
  const text = line.replace(CODE, "");
  if ((text.match(/\*\*/g)?.length ?? 0) % 2 !== 0) return "a **bold** is not closed";
  for (const [, , target = ""] of text.matchAll(LINK)) {
    const topic = /^help:(.+)$/.exec(target)?.[1];
    if (topic !== undefined ? !isHelpTopic(topic) : !/^route:\/\S*$/.test(target)) {
      return `the link '${target}' must be help:<topic> or route:/<path>`;
    }
  }
  const rest = text.replace(/^- \[ \] /, "").replace(LINK, "");
  if (rest.includes("[") || rest.includes("]"))
    return "only [text](help:…) and [text](route:/…) links are allowed";
  return null;
}

/**
 * The help subset: `# ` title (exactly one, the first line), `## ` sections, paragraphs, `- ` and
 * `1. ` lists, `- [ ] ` checklist items, `> ` tips, `**bold**`, `` `F2` `` keys and
 * `[text](help:<topic>)` / `[text](route:/<path>)` links. Anything else is a problem (`line N: …`).
 */
export function parseHelp(text: string): { blocks: Block[]; problems: string[] } {
  const blocks: Block[] = [];
  const problems: string[] = [];
  let current: Block | null = null;
  const close = () => {
    if (current) blocks.push(current);
    current = null;
  };
  const addItem = (kind: "list" | "ordered" | "checklist", item: string) => {
    if (current?.kind !== kind) {
      close();
      current = { kind, items: [] };
    }
    current.items.push(item.trim());
  };
  const addText = (kind: "paragraph" | "tip", line: string) => {
    if (current?.kind === kind) current.text = `${current.text} ${line.trim()}`;
    else {
      close();
      current = { kind, text: line.trim() };
    }
  };
  text
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .forEach((line, index) => {
      const number = index + 1;
      if (index === 0 && !line.startsWith("# ")) problems.push("line 1: the page must start with '# '");
      const problem = lineProblem(line);
      if (problem) {
        problems.push(`line ${number}: ${problem}`);
        return;
      }
      if (line.trim() === "") close();
      else if (line.startsWith("# ")) {
        if (index !== 0) problems.push(`line ${number}: only the first line is a '# ' title`);
        close();
        blocks.push({ kind: "title", text: line.slice(2).trim() });
      } else if (line.startsWith("## ")) {
        close();
        blocks.push({ kind: "section", text: line.slice(3).trim() });
      } else if (line.startsWith("- [ ] ")) addItem("checklist", line.slice(6));
      else if (line.startsWith("- ")) addItem("list", line.slice(2));
      else if (ORDERED.test(line)) addItem("ordered", line.replace(ORDERED, ""));
      else if (line.startsWith("> ")) addText("tip", line.slice(2));
      else addText("paragraph", line);
    });
  close();
  return { blocks, problems };
}

interface Links {
  onTopic: (topic: string) => void;
  /** Absent outside a router (the setup wizard's tour): a screen link is then plain text. */
  onRoute?: ((path: string) => void) | undefined;
}

const INLINE = /\*\*(.+?)\*\*|`([^`]+)`|\[([^\]]+)\]\(([^)]+)\)/g;

/** A file name or extension (`.dzpos`, `pos.db`), a path (`C:\\…`) or a Windows variable
 * (`%APPDATA%`) in running text. */
const LATIN_NAME = /(?:[A-Za-z]:\\\S*|%[A-Za-z_]+%\S*|[\w-]*\.[A-Za-z][A-Za-z0-9]{1,4}\b)/g;

/** Plain text with each file name, path or variable isolated left to right (finding 24): in an
 * Arabic sentence ".dzpos" would otherwise show as "dzpos." and "(.dzpos)" as "(dzpos.)". */
function isolateNames(text: string, at: number): ReactNode[] {
  const nodes: ReactNode[] = [];
  let last = 0;
  for (const match of text.matchAll(LATIN_NAME)) {
    if (match.index > last) nodes.push(text.slice(last, match.index));
    nodes.push(
      <bdi key={`${at}-${match.index}`} dir="ltr">
        {match[0]}
      </bdi>,
    );
    last = match.index + match[0].length;
  }
  if (last < text.length) nodes.push(text.slice(last));
  return nodes;
}

/** `**bold**`, `` `key` `` and links as React elements (never `innerHTML`). */
function inline(text: string, links: Links): ReactNode[] {
  const nodes: ReactNode[] = [];
  let last = 0;
  for (const match of text.matchAll(INLINE)) {
    const [whole, bold, key, label = "", target = ""] = match;
    if (match.index > last) nodes.push(...isolateNames(text.slice(last, match.index), last));
    const id = match.index;
    if (bold !== undefined) nodes.push(<strong key={id}>{bold}</strong>);
    else if (key !== undefined) {
      nodes.push(
        <kbd key={id} dir="ltr" className="rounded-sm border border-line px-1.5 font-semibold">
          {key}
        </kbd>,
      );
    } else {
      const { onTopic, onRoute } = links;
      const topic = target.startsWith("help:") ? target.slice("help:".length) : null;
      const route = target.startsWith("route:") ? target.slice("route:".length) : null;
      const go =
        topic !== null ? () => onTopic(topic) : route !== null && onRoute ? () => onRoute(route) : null;
      nodes.push(
        go ? (
          // data-help-link: the link's words stay on paper when the page is printed (print.css)
          <button key={id} type="button" data-help-link="" className="text-info underline" onClick={go}>
            {label}
          </button>
        ) : (
          <span key={id}>{label}</span>
        ),
      );
    }
    last = match.index + whole.length;
  }
  if (last < text.length) nodes.push(...isolateNames(text.slice(last), last));
  return nodes;
}

/** The page-wide index of each block's first checklist item, whatever section it sits in. */
function checklistStarts(blocks: readonly Block[]): number[] {
  const starts: number[] = [];
  let next = 0;
  for (const block of blocks) {
    starts.push(next);
    if (block.kind === "checklist") next += block.items.length;
  }
  return starts;
}

function readTicks(storageKey: string): number[] {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(storageKey) ?? "[]");
    return Array.isArray(saved) ? saved.filter((n): n is number => Number.isInteger(n)) : [];
  } catch {
    // storage unavailable: ticks last until the page closes
    return [];
  }
}

/** A help page as React elements; the ticks of its checklist stay on this PC under
 * `dzpos.help.<checklistKey>` (the same items in the three languages). */
export function HelpMarkdown({
  text,
  onTopic,
  onRoute,
  checklistKey,
}: {
  text: string;
  checklistKey: string;
} & Links) {
  const storageKey = `dzpos.help.${checklistKey}`;
  const { blocks, firstItem } = useMemo(() => {
    const parsed = parseHelp(text).blocks;
    return { blocks: parsed, firstItem: checklistStarts(parsed) };
  }, [text]);
  const [ticks, setTicks] = useState<number[]>(() => readTicks(storageKey));
  const base = useId();
  const links: Links = { onTopic, onRoute };
  const toggle = (index: number, checked: boolean) => {
    const next = ticks.filter((n) => n !== index);
    if (checked) next.push(index);
    setTicks(next);
    try {
      localStorage.setItem(storageKey, JSON.stringify(next));
    } catch {
      // storage unavailable: ticks last until the page closes
    }
  };
  return blocks.map((block, b) => {
    switch (block.kind) {
      case "title":
        return (
          <h1 key={b} className="text-3xl font-bold">
            {inline(block.text, links)}
          </h1>
        );
      case "section":
        return (
          <h2 key={b} className="mt-2 text-xl font-bold">
            {inline(block.text, links)}
          </h2>
        );
      case "paragraph":
        return <p key={b}>{inline(block.text, links)}</p>;
      case "tip":
        return (
          <aside key={b} className="rounded-control border-s-4 border-info bg-info-soft p-3">
            {inline(block.text, links)}
          </aside>
        );
      case "list":
      case "ordered": {
        const List = block.kind === "list" ? "ul" : "ol";
        return (
          <List
            key={b}
            className={cn("space-y-1 ps-6", block.kind === "list" ? "list-disc" : "list-decimal")}
          >
            {block.items.map((item, i) => (
              <li key={i}>{inline(item, links)}</li>
            ))}
          </List>
        );
      }
      case "checklist":
        return (
          <ul key={b} role="list" className="flex flex-col gap-1">
            {block.items.map((item, i) => {
              const index = (firstItem[b] ?? 0) + i;
              const id = `${base}-${index}`;
              // the words sit beside the box, not inside its <label>: a link in them is a button,
              // and a label may not hold a button
              return (
                <li key={index} className="flex items-start gap-1">
                  <label htmlFor={id} className="grid size-11 shrink-0 place-items-center">
                    <input
                      id={id}
                      type="checkbox"
                      className="size-5 accent-brand"
                      aria-labelledby={`${id}-text`}
                      checked={ticks.includes(index)}
                      onChange={(event) => toggle(index, event.target.checked)}
                    />
                  </label>
                  <span id={`${id}-text`} className="pt-2.5">
                    {inline(item, links)}
                  </span>
                </li>
              );
            })}
          </ul>
        );
    }
  });
}
