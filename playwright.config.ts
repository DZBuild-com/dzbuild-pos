import { defineConfig } from "@playwright/test";

/** The E2E suite (M6 Task 19): the real UI in Edge (the engine of WebView2) over the fake backend
 * of `src/test/e2e`, served by Vite with `VITE_E2E=1`. Screenshots and traces go to
 * `e2e-artifacts/` (git-ignored). */
export default defineConfig({
  testDir: "e2e",
  outputDir: "e2e-artifacts/results",
  fullyParallel: false,
  workers: 1,
  timeout: 120_000,
  retries: process.env.CI ? 1 : 0,
  // The dev server transforms a lazy screen or the Arabic and English texts on their first visit
  // (vite.config.ts warms them up at start); on a cold server that passed the 5 s default.
  expect: { timeout: 15_000 },
  reporter: [["list"], ["html", { outputFolder: "e2e-artifacts/report", open: "never" }]],
  use: {
    baseURL: "http://localhost:1430",
    channel: "msedge",
    viewport: { width: 1366, height: 768 },
    // A French Windows, as the unit tests: a first start opens in Windows' language when it is
    // Arabic or English (`src/features/setup/setup-lang.ts`).
    locale: "fr-FR",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  webServer: {
    command: "pnpm exec vite --port 1430 --strictPort",
    url: "http://localhost:1430",
    reuseExistingServer: !process.env.CI,
    env: { VITE_E2E: "1" },
    timeout: 120_000,
  },
});
