import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";

const read = (file) => readFileSync(file, "utf8");
const WORKFLOWS = ".github/workflows";
const workflows = readdirSync(WORKFLOWS)
  .filter((f) => f.endsWith(".yml"))
  .map((f) => ({ file: f, text: read(`${WORKFLOWS}/${f}`) }));

test("a push to main runs the frontend, Rust and cargo-deny gates side by side, never a pull request", () => {
  const ci = read(`${WORKFLOWS}/ci.yml`);
  assert.doesNotMatch(ci, /pull_request:/, "pull requests are built by hand, not automatically");
  assert.match(ci, /branches: \[main\]/);
  assert.match(ci, /workflow_call:/, "release.yml reuses the CI gates");
  for (const job of ["frontend:", "rust:", "deny:"]) assert.ok(ci.includes(`\n  ${job}`), job);
  for (const step of [
    "pnpm install --frozen-lockfile",
    "pnpm lint",
    "pnpm typecheck",
    "pnpm test",
    "pnpm test:tooling",
    "pnpm i18n",
    "prettier --check",
    "pnpm perf:bundle",
    "cargo fmt --all --check",
    "cargo clippy --workspace --all-targets -- -D warnings",
    "cargo test --workspace",
    "--target i686-pc-windows-msvc",
    "cargo-deny-action",
  ]) {
    assert.ok(ci.includes(step), `ci.yml lacks ${step}`);
  }
  assert.match(ci, /cancel-in-progress: true/);
});

test("a tag builds the 64-bit and 32-bit installers and drafts a release, never printing a secret", () => {
  const release = read(`${WORKFLOWS}/release.yml`);
  assert.match(release, /tags:\s*\n\s*- "v\*"/);
  assert.match(release, /workflow_dispatch:/);
  assert.match(release, /options: \[both, x64, x86\]/);
  assert.match(release, /prerelease:\s*\n\s*description:/);
  assert.match(release, /uses: \.\/\.github\/workflows\/ci\.yml/);
  assert.match(release, /matrix\.arch == 'x86'/);
  assert.match(release, /rustup target add i686-pc-windows-msvc/);
  assert.match(release, /pnpm release:build --arch/);
  assert.match(release, /name: installers-\$\{\{ matrix\.arch \}\}/);
  assert.match(release, /scripts\/release\/collect\.mjs/);
  assert.match(release, /pnpm release:manifest/);
  assert.match(release, /scripts\/release\/notes-for-tag\.mjs/);
  assert.match(release, /TAURI_SIGNING_PRIVATE_KEY: \$\{\{ secrets\.TAURI_SIGNING_PRIVATE_KEY \}\}/);
  assert.match(release, /gh release create .*--draft/);
  assert.match(release, /already published/);
  // contents: write only on the job that drafts the release.
  assert.match(release, /^permissions:\s*\n\s*contents: read/m);
  assert.equal(release.match(/contents: write/g)?.length, 1);
});

test("every workflow has least-privilege permissions, pinned actions and no printed secret", () => {
  for (const { file, text } of workflows) {
    assert.match(text, /^permissions:/m, `${file} sets no top-level permissions`);
    assert.ok(!/(echo|Write-(Output|Host)) .*secrets\./.test(text), `${file} prints a secret`);
    for (const line of text.split("\n").filter((l) => /^\s*-?\s*uses:/.test(l))) {
      const ref = line.split("uses:")[1].trim();
      if (ref.startsWith("./")) continue;
      // dtolnay/rust-toolchain is versioned by toolchain name (its documented usage).
      const pinned = /@v\d+(\.\d+)*$/.test(ref) || ref === "dtolnay/rust-toolchain@stable";
      assert.ok(pinned, `${file}: ${ref} is not pinned to a release tag`);
    }
  }
});

test("security scanning: CodeQL, TruffleHog, Scorecard, dependency review, labeler, Dependabot", () => {
  const codeql = read(`${WORKFLOWS}/codeql.yml`);
  for (const language of ["javascript-typescript", "rust", "actions"]) assert.ok(codeql.includes(language));
  const secrets = read(`${WORKFLOWS}/secret-scan.yml`);
  assert.match(secrets, /trufflesecurity\/trufflehog@/);
  assert.match(secrets, /fetch-depth: 0/);
  assert.match(secrets, /filesystem/);
  assert.match(secrets, /--only-verified/);
  assert.match(read(`${WORKFLOWS}/scorecard.yml`), /publish_results: true/);
  assert.match(read(`${WORKFLOWS}/dependency-review.yml`), /dependency-review-action@/);
  assert.match(read(`${WORKFLOWS}/labeler.yml`), /actions\/labeler@/);
  assert.ok(existsSync(".github/labeler.yml"));
  const dependabot = read(".github/dependabot.yml");
  for (const ecosystem of ["npm", "cargo", "github-actions"]) {
    assert.ok(dependabot.includes(`package-ecosystem: ${ecosystem}`), ecosystem);
  }
  assert.equal(dependabot.match(/interval: weekly/g)?.length, 3);
  assert.match(dependabot, /update-types: \[minor, patch\]/);
  assert.match(read("deny.toml"), /\[licenses\]/);
});

test("the nightly suite runs the E2E suite, the budgets and the audits, only when started by hand", () => {
  const nightly = read(`${WORKFLOWS}/nightly.yml`);
  assert.doesNotMatch(nightly, /schedule:/, "nothing runs on a timer");
  assert.match(nightly, /workflow_dispatch:/);
  assert.match(nightly, /pnpm e2e/);
  assert.match(nightly, /e2e-artifacts/);
  assert.match(nightly, /budget_/);
  assert.match(nightly, /pnpm audit --prod/);
  assert.match(nightly, /cargo-deny-action/);
  assert.ok(existsSync(".github/dependabot.yml") && existsSync("deny.toml"));
});
