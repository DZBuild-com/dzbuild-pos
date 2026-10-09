# Data-pack test fixtures

`test-only.key` signs test fixtures only; it is not trusted by the app (the app trusts
`crates/pos-core/data/datapack.pub`, whose private key stays on the release PC, kept like the
updater key: `docs/release/updater-key.md`).

- `test-only.key` / `test-only.key.pub`: a minisign key pair made with
  `pnpm tauri signer generate --ci -p '' -w crates/pos-core/tests/fixtures/datapack/test-only.key`.
- `dz-datapack-test-v2.json`: the bundled 2026 pack with `id` `dz-2026.2-test`, `version` 2,
  `validFrom` `2026-09-01` and the 1 L oil ceiling at `13000`; `.sig` is its signature by the test
  key (`pnpm tauri signer sign -f crates/pos-core/tests/fixtures/datapack/test-only.key -p ''
  crates/pos-core/tests/fixtures/datapack/dz-datapack-test-v2.json`).
- `dz-datapack-test-broken.json`: `{}`, signed the same way: a file the right key signed that is
  not a pack (`datapack_file_invalid`).

The signature covers the exact bytes of the JSON file: never reformat it. Edit the copy, then sign
it again.
