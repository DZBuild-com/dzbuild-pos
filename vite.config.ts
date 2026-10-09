/// <reference types="vitest/config" />
import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

const host = process.env.TAURI_DEV_HOST;
/** The E2E run: playwright.config.ts starts this dev server with `VITE_E2E=1`. */
const e2e = process.env.VITE_E2E === "1";

/** NFR-002: fontsource's `@font-face` rules list a WOFF fallback after each WOFF2 file. WebView2
 * always takes the WOFF2 one, so the fallbacks would only add 0.29 MB to the online setup: they are
 * cut before Vite resolves the URLs, and never emitted (`pnpm perf:bundle` refuses a `.woff`). */
function woff2Only(): Plugin {
  return {
    name: "dzpos:woff2-only",
    enforce: "pre",
    transform(code, id) {
      // Vite's ids use forward slashes on Windows too.
      if (!/\/@fontsource\/[^?]+\.css(\?|$)/.test(id)) return null;
      return { code: code.replace(/,\s*url\([^)]*\.woff\)\s*format\(["']?woff["']?\)/g, ""), map: null };
    },
  };
}

export default defineConfig({
  plugins: [woff2Only(), react(), tailwindcss()],
  resolve: { alias: { "@": path.resolve(import.meta.dirname, "src") } },
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_ENV_"],
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    // E2E: no file watching. An edit anywhere in the tree (a doc included: Tailwind scans it)
    // would reload the page under test and lose the fake shop, which lives in that page.
    watch: e2e ? null : { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] },
    // E2E only: transform what the app loads on demand (the fake backend, the texts of each
    // language, the lazy screens) as soon as the server starts, so the first visit of a screen does
    // not wait on a cold transform. Tests are never warmed: their imports are not browser deps.
    warmup: e2e
      ? {
          clientFiles: [
            "./src/main.tsx",
            "./src/test/e2e/install.ts",
            "./src/locales/bundles/*.ts",
            "./src/features/**/*.tsx",
            "!./src/**/*.test.tsx",
          ],
        }
      : undefined,
  },
  build: {
    target: "chrome120",
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
    // `dist/.vite/manifest.json`: `pnpm perf:bundle` measures what the /pos route loads (NFR-010).
    manifest: true,
    // The app, the hidden print window's page (print-host.html, spec §9.2) and the customer
    // screen, its own page whose bundle holds React, i18next and the display only.
    rolldownOptions: {
      input: {
        main: path.resolve(import.meta.dirname, "index.html"),
        printHost: path.resolve(import.meta.dirname, "print-host.html"),
        display: path.resolve(import.meta.dirname, "customer-display.html"),
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    css: false,
    restoreMocks: true,
    // Whole-screen tests under a full parallel run outlive the 5 s default on a loaded machine
    // (other lanes compile Rust on the same PC): one global budget, never per-file patches.
    testTimeout: 30_000,
    hookTimeout: 30_000,
  },
});
