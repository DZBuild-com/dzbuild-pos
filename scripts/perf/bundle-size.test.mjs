import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { checkBundle } from "./bundle-size.mjs";

function dist(files, manifest) {
  const root = mkdtempSync(path.join(tmpdir(), "dzpos-dist-"));
  for (const [name, content] of Object.entries(files)) {
    const full = path.join(root, name);
    mkdirSync(path.dirname(full), { recursive: true });
    writeFileSync(full, content);
  }
  mkdirSync(path.join(root, ".vite"), { recursive: true });
  writeFileSync(path.join(root, ".vite", "manifest.json"), JSON.stringify(manifest));
  return root;
}

const lazy = {
  "src/features/products/ProductsScreen.tsx": {
    file: "assets/products.js",
    src: "src/features/products/ProductsScreen.tsx",
    isDynamicEntry: true,
  },
};

test("a small entry with lazy screens passes", () => {
  const root = dist(
    { "assets/index.js": "x".repeat(1000), "assets/vendor.js": "y".repeat(1000), "assets/products.js": "z" },
    {
      "index.html": { file: "assets/index.js", isEntry: true, imports: ["_vendor.js"] },
      "_vendor.js": { file: "assets/vendor.js" },
      ...lazy,
    },
  );
  const report = checkBundle(root, { lazyScreens: ["src/features/products/ProductsScreen.tsx"] });
  assert.deepEqual(report.problems, []);
  assert.ok(report.posGzipBytes > 0 && report.posGzipBytes < 1000);
});

test("an entry over 300 kB gzipped, an eager screen or the E2E backend is refused", () => {
  const random = randomBytes(400_000).toString("base64"); // incompressible: ~400 kB gzipped
  const root = dist(
    { "assets/index.js": random, "assets/fake.js": "const m = '__DZPOS_E2E_FAKE_BACKEND__';" },
    { "index.html": { file: "assets/index.js", isEntry: true, imports: [] }, x: { file: "assets/fake.js" } },
  );
  const report = checkBundle(root, { lazyScreens: ["src/features/products/ProductsScreen.tsx"] });
  assert.equal(report.problems.length, 3, report.problems.join("\n"));
  assert.match(report.problems.join("\n"), /300 kB/);
  assert.match(report.problems.join("\n"), /ProductsScreen\.tsx is not lazy/);
  assert.match(report.problems.join("\n"), /E2E fake backend/);
});

test("the /pos figure is the entry plus the largest language's till texts (finding 28)", () => {
  const texts = (n) => randomBytes(n).toString("base64"); // incompressible
  const files = {
    "assets/index.js": texts(200_000),
    "assets/ar-till.js": texts(120_000),
    "assets/fr-till.js": texts(30_000),
    "assets/ar-office.js": texts(200_000),
  };
  const manifest = {
    "index.html": { file: "assets/index.js", isEntry: true, imports: [] },
    "src/locales/bundles/ar-till.ts": {
      file: "assets/ar-till.js",
      src: "src/locales/bundles/ar-till.ts",
      isDynamicEntry: true,
    },
    "src/locales/bundles/fr-till.ts": {
      file: "assets/fr-till.js",
      src: "src/locales/bundles/fr-till.ts",
      isDynamicEntry: true,
    },
    "src/locales/bundles/ar-office.ts": {
      file: "assets/ar-office.js",
      src: "src/locales/bundles/ar-office.ts",
      isDynamicEntry: true,
    },
  };
  const report = checkBundle(dist(files, manifest), { lazyScreens: [] });
  assert.ok(report.tillTexts.ar > report.tillTexts.fr, JSON.stringify(report.tillTexts));
  assert.equal(report.posGzipBytes, report.entryGzipBytes + report.tillTexts.ar);
  assert.equal(report.tillTexts.office, undefined, "the back office's texts never count");
  // ~200 kB + ~120 kB: the Arabic till is over budget although the entry alone is not.
  assert.ok(report.entryGzipBytes < 300 * 1024);
  assert.match(report.problems.join("\n"), /over 300 kB/);
  assert.match(report.problems.join("\n"), / in ar,/);
});

test("the till's pack chunks are measured apart and kept under 120 kB gzipped", () => {
  const texts = (n) => randomBytes(n).toString("base64"); // incompressible
  const pack = "src/features/services/ServicePickup.tsx";
  const files = {
    "assets/index.js": texts(10_000),
    "assets/pickup.js": texts(20_000),
    "assets/shared.js": texts(5_000),
    "assets/vendor.js": texts(1_000),
  };
  const manifest = {
    "index.html": { file: "assets/index.js", isEntry: true, imports: ["_vendor.js"] },
    "_vendor.js": { file: "assets/vendor.js" },
    "_shared.js": { file: "assets/shared.js" },
    [pack]: {
      file: "assets/pickup.js",
      src: pack,
      isDynamicEntry: true,
      imports: ["_shared.js", "_vendor.js"],
    },
  };
  const small = checkBundle(dist(files, manifest), { lazyScreens: [pack], packScreens: [pack] });
  assert.deepEqual(small.problems, []);
  // the pack's own chunk and what it shares with no other part of the entry, never the entry's
  assert.ok(small.packGzipBytes > 20_000 && small.packGzipBytes < 30_000, String(small.packGzipBytes));
  const big = { ...files, "assets/pickup.js": texts(200_000) };
  const over = checkBundle(dist(big, manifest), { lazyScreens: [pack], packScreens: [pack] });
  assert.match(over.problems.join("\n"), /pack chunks .* over 120 kB/);
});

test("each till pack screen is reported beyond the /pos route, the shared entry not counted again", () => {
  const bytes = (n) => randomBytes(n).toString("base64"); // incompressible
  const files = {
    "assets/index.js": "x".repeat(1000),
    "assets/floor.js": bytes(20_000),
    "assets/ticket.js": bytes(160_000),
    "assets/shared.js": "y".repeat(1000),
  };
  const screen = (name, file) => ({ file, src: `src/features/restaurant/${name}.tsx`, isDynamicEntry: true });
  const manifest = {
    "index.html": { file: "assets/index.js", isEntry: true, imports: ["_shared.js"] },
    "_shared.js": { file: "assets/shared.js" },
    "src/features/restaurant/FloorScreen.tsx": {
      ...screen("FloorScreen", "assets/floor.js"),
      imports: ["_shared.js"],
    },
    "src/features/restaurant/TicketScreen.tsx": screen("TicketScreen", "assets/ticket.js"),
  };
  const report = checkBundle(dist(files, manifest), {
    lazyScreens: [],
    packScreens: ["src/features/restaurant/FloorScreen.tsx", "src/features/restaurant/TicketScreen.tsx"],
  });
  const floor = report.packScreens["src/features/restaurant/FloorScreen.tsx"];
  assert.ok(floor > 20_000 && floor < 30_000, `the entry's shared chunk is not counted again: ${floor}`);
  assert.equal(report.problems.length, 1, report.problems.join("\n"));
  // one budget for the pack chunks together: 20 kB + 160 kB is over 120 kB
  assert.match(report.problems.join("\n"), /pack chunks .* over 120 kB/);
});

test("fonts ship as WOFF2 only: a WOFF fallback in the build is refused (NFR-002)", () => {
  const manifest = { "index.html": { file: "assets/index.js", isEntry: true, imports: [] } };
  const woff2 = dist({ "assets/index.js": "x", "assets/inter-latin-400-normal-a1.woff2": "w2" }, manifest);
  assert.deepEqual(checkBundle(woff2, { lazyScreens: [], packScreens: [] }).problems, []);
  const both = dist(
    {
      "assets/index.js": "x",
      "assets/inter-latin-400-normal-a1.woff2": "w2",
      "assets/inter-latin-400-normal-b2.woff": "w1",
    },
    manifest,
  );
  const problems = checkBundle(both, { lazyScreens: [], packScreens: [] }).problems;
  assert.equal(problems.length, 1, problems.join("\n"));
  assert.match(problems[0], /assets\/inter-latin-400-normal-b2\.woff: a WOFF font/);
});
