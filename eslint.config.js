// @ts-check
import js from "@eslint/js";
import { defineConfig, globalIgnores } from "eslint/config";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

/**
 * Physical-direction Tailwind classes break Arabic RTL (spec §2, §14).
 * esquery strips backslashes inside selector regexes, so the pattern avoids \s and \w.
 */
export const PHYSICAL_DIRECTION =
  "/(^|[ :])-?(ml|mr|pl|pr|left|right|scroll-ml|scroll-mr|scroll-pl|scroll-pr)-|(^|[ :])(text-left|text-right|float-left|float-right)($|[^a-z0-9-])|(^|[ :])(rounded-(l|r|tl|tr|bl|br)|border-(l|r))($|[^a-z0-9])/";

const directionMessage =
  "Physical direction class breaks Arabic RTL. Use ms-/me-/ps-/pe-/start-/end-/text-start/text-end/rounded-s/rounded-e/border-s/border-e.";

export default defineConfig([
  globalIgnores([
    "node_modules/**",
    "dist/**",
    "target/**",
    "coverage/**",
    "e2e-artifacts/**",
    "src-tauri/target/**",
    "src-tauri/gen/**",
    "src/bindings.ts",
    "src/bindings-packs.ts",
    "assets/**",
    ".worktrees/**",
  ]),
  {
    files: ["**/*.{js,mjs,ts,tsx}"],
    extends: [js.configs.recommended, tseslint.configs.recommended],
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    extends: [reactHooks.configs.flat.recommended],
    rules: {
      "no-restricted-syntax": [
        "error",
        { selector: `Literal[value=${PHYSICAL_DIRECTION}]`, message: directionMessage },
        { selector: `TemplateElement[value.raw=${PHYSICAL_DIRECTION}]`, message: directionMessage },
      ],
      "no-restricted-imports": [
        "error",
        {
          paths: [
            {
              name: "@tauri-apps/api/core",
              importNames: ["invoke"],
              message: "Call the backend through src/lib/ipc/* hooks, never invoke().",
            },
          ],
          patterns: [
            {
              group: ["@/bindings", "**/bindings", "@/bindings-packs", "**/bindings-packs"],
              message: "Only src/lib/ipc/* may import the generated bindings.",
            },
          ],
        },
      ],
    },
  },
  {
    files: ["src/lib/ipc/**/*.ts", "src/**/*.test.{ts,tsx}", "src/test/**"],
    rules: { "no-restricted-imports": "off" },
  },
  // The Playwright suite runs in Node (its page callbacks in the browser).
  { files: ["e2e/**/*.ts", "playwright.config.ts"], languageOptions: { globals: globals.node } },
]);
