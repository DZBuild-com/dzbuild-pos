import assert from "node:assert/strict";
import { test } from "node:test";
import { ESLint } from "eslint";

const eslint = new ESLint({ cwd: process.cwd() });

async function directionErrors(code) {
  const [result] = await eslint.lintText(code, { filePath: "src/__direction_probe__.tsx" });
  return result.messages.filter((m) => m.ruleId === "no-restricted-syntax").length;
}

test("physical direction classes are rejected", async () => {
  for (const cls of [
    "ml-2",
    "hover:pr-3",
    "text-left",
    "rounded-l-md",
    "border-r",
    "-mr-1",
    "left-0",
    "rounded-tl-lg",
  ]) {
    assert.equal(await directionErrors(`export const c = "p-4 ${cls}";\n`), 1, cls);
  }
});

test("physical classes inside template literals are rejected", async () => {
  assert.equal(await directionErrors("export const c = `flex ${String(1)} rounded-l-md`;\n"), 1);
});

test("logical classes are accepted", async () => {
  for (const cls of [
    "ms-2 me-1 ps-4 pe-2",
    "text-start text-end",
    "rounded-s-md rounded-e border-s border-e start-0 end-0",
    "rounded-lg border-line border text-lg",
    "rtl:-scale-x-100 mx-auto inset-x-0",
  ]) {
    assert.equal(await directionErrors(`export const c = "${cls}";\n`), 0, cls);
  }
});
