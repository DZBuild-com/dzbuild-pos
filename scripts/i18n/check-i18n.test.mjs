import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { checkI18n } from "./check-i18n.mjs";

function project(files) {
  const root = mkdtempSync(path.join(tmpdir(), "dzpos-i18n-"));
  for (const [relative, content] of Object.entries(files)) {
    const full = path.join(root, relative);
    mkdirSync(path.dirname(full), { recursive: true });
    writeFileSync(full, JSON.stringify(content));
  }
  return root;
}

function rawProject(files) {
  const root = mkdtempSync(path.join(tmpdir(), "dzpos-help-"));
  for (const [relative, content] of Object.entries(files)) {
    const full = path.join(root, relative);
    mkdirSync(path.dirname(full), { recursive: true });
    writeFileSync(full, content);
  }
  return root;
}

const ui = {
  "src/locales/ar/common.json": { save: "حفظ" },
  "src/locales/fr/common.json": { save: "Enregistrer" },
  "src/locales/en/common.json": { save: "Save" },
};

test("document catalogues in sync pass", () => {
  const root = project({
    ...ui,
    "crates/pos-docs/locales/ar.json": { doc: { ticket: "تذكرة الصندوق" } },
    "crates/pos-docs/locales/fr.json": { doc: { ticket: "Ticket de caisse" } },
    "crates/pos-docs/locales/en.json": { doc: { ticket: "Receipt" } },
  });
  assert.deepEqual(checkI18n(root), []);
});

test("a missing document key is reported", () => {
  const root = project({
    ...ui,
    "crates/pos-docs/locales/ar.json": { doc: {} },
    "crates/pos-docs/locales/fr.json": { doc: { ticket: "Ticket de caisse" } },
    "crates/pos-docs/locales/en.json": { doc: { ticket: "Receipt" } },
  });
  const problems = checkI18n(root);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /ar\/pos-docs missing keys: doc\.ticket/);
});

test("empty values and a missing language are reported", () => {
  const root = project({
    ...ui,
    "crates/pos-docs/locales/ar.json": { doc: { ticket: "" } },
    "crates/pos-docs/locales/fr.json": { doc: { ticket: "Ticket de caisse" } },
  });
  const problems = checkI18n(root);
  assert.ok(
    problems.some((p) => /en\.json missing/.test(p)),
    problems.join("\n"),
  );
  assert.ok(
    problems.some((p) => /ar\/pos-docs empty values: doc\.ticket/.test(p)),
    problems.join("\n"),
  );
});

const helpPage = (title) => `# ${title}\n\n## Un\n\ntexte\n\n## Deux\n\n- [ ] étape\n`;

test("help pages in sync pass", () => {
  const root = rawProject({
    "src/help/ar/pos.md": helpPage("البيع"),
    "src/help/fr/pos.md": helpPage("Vendre"),
    "src/help/en/pos.md": helpPage("Sell"),
  });
  assert.deepEqual(checkI18n(root), []);
});

test("a help page missing in one language is reported", () => {
  const root = rawProject({
    "src/help/fr/pos.md": helpPage("Vendre"),
    "src/help/en/pos.md": helpPage("Sell"),
  });
  const problems = checkI18n(root);
  assert.ok(
    problems.some((p) => /src\/help\/ar\/pos\.md missing/.test(p)),
    problems.join("\n"),
  );
});

test("a help page with other sections or an empty file is reported", () => {
  const root = rawProject({
    "src/help/ar/pos.md": "# البيع\n\n## واحد\n",
    "src/help/fr/pos.md": helpPage("Vendre"),
    "src/help/en/pos.md": "",
  });
  const problems = checkI18n(root);
  assert.ok(
    problems.some((p) => /ar\/pos\.md has 1 sections, fr has 2/.test(p)),
    problems.join("\n"),
  );
  assert.ok(
    problems.some((p) => /en\/pos\.md is empty/.test(p)),
    problems.join("\n"),
  );
});
