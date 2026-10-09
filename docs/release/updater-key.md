# The updater key

Every update is checked by the installed app against the public key in
`src-tauri/tauri.conf.json` (`plugins.updater.pubkey`). The matching private key signs the
update setups. **Losing it means no installed copy can ever update automatically again.**

## Where it is

- `src-tauri/keys/updater.key` (the private key, encrypted with its password),
- `src-tauri/keys/updater.password`,
- `src-tauri/keys/updater.key.pub` (public, also in `tauri.conf.json`).

`src-tauri/keys/` is ignored by git; never copy it into the repository, a ticket or a chat.

The pair was made in M6 inside the release lane's worktree,
`.worktrees\m6app\src-tauri\keys\`. `git worktree remove` deletes ignored files: before that
worktree goes, copy the three files into the main checkout's `src-tauri\keys\` (where
`pnpm release:build` looks for them) and onto the sticks below:
`Copy-Item .worktrees\m6app\src-tauri\keys\updater.* src-tauri\keys\`.

## Custody (owner, once)

1. Copy the three files to two USB sticks kept in two different places (for example the shop's
   safe and home), and the password into a password manager.
2. Add them as CI secrets (PowerShell pipes the files; nothing is printed):
   `Get-Content -Raw src-tauri\keys\updater.key | gh secret set TAURI_SIGNING_PRIVATE_KEY` and
   `Get-Content -Raw src-tauri\keys\updater.password | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
3. Check once a year that a stick still opens.

## Rotation (a key lost or leaked)

A leaked key: an attacker could sign a setup, but still needs to serve it from the manifest URL
and to pass Authenticode. Rotate at once:

1. `pnpm tauri signer generate -w src-tauri\keys\updater-new.key -p <new password> --ci`.
2. Release N is built and signed with the **old** key and carries the **new** public key
   (`node scripts/release/set-pubkey.mjs --pub src-tauri\keys\updater-new.key.pub`); every copy
   that installs N trusts the new key from then on.
3. Releases N+1 and later are signed with the new key; replace the files (`updater-new.key` and
   `updater-new.key.pub` become `updater.key` and `updater.key.pub`, the new password goes into
   `updater.password`), the sticks and the CI secrets.

A lost key: copies can no longer be updated by the updater; publish a new version signed with a
new key as a full setup on dzbuild.com/pos and announce it (the installed data stays in place).
