# Hosting the downloads and the update manifests

## URL map (2026-10-09)

Every link a shop user sees goes to the marketing site `https://dzbuild.com`, under `/pos`:

| What | URL | Used by |
|---|---|---|
| Product and download page | `https://dzbuild.com/pos` | the app's "Télécharger" buttons, `homepage` in `tauri.conf.json`, `url` in `src-tauri/license.toml` |
| Setup files | `https://dzbuild.com/pos/download/<version>/<file>` | the update manifests (`pnpm release:manifest --base`, default `https://dzbuild.com/pos/download`) |
| Support page | `https://dzbuild.com/pos/support` | the default `support.contact` setting (help centre, Réglages › Diagnostic) |
| Update manifest | `https://dzbuild.com/pos/updates/<channel>/latest.json` (`stable`, `beta`) | `plugins.updater.endpoints` in `tauri.conf.json` |
| DZBuild itself (online store, account) | `https://dzbuild.com`, sign-up `https://dzbuild.com/register` | the optional account link (`src-tauri/dzbuild.toml`) |
| DZBuild blog | `https://blog.dzbuild.com` | marketing only |
| DZBuild API | `https://api.dzbuild.app/v1` | the optional account link (`src-tauri/dzbuild.toml`) |

`dzbuild.dev` is the developer documentation only: it is never shown to shop users. The app checks
only that an update endpoint is HTTPS and names a `/stable/` folder, so a manifest or setup still
served from an older host keeps validating; new releases use `dzbuild.com`.

## Layout

Layout on `https://dzbuild.com` (any static host with HTTPS works; change the URL in
`src-tauri/tauri.conf.json` > `plugins.updater.endpoints` and pass `--base` to
`pnpm release:manifest` if it moves; the endpoint must keep `/stable/` in its path, the app reads
the beta channel from `/beta/` next to it):

    /pos/download/<version>/DZBuildPOS-<version>-setup.exe
    /pos/download/<version>/DZBuildPOS-<version>-offline-setup.exe
    /pos/download/<version>/DZBuildPOS-<version>-x86-setup.exe           (32-bit Windows, 1.0.0)
    /pos/download/<version>/DZBuildPOS-<version>-x86-offline-setup.exe
    /pos/download/<version>/SHA256SUMS.txt
    /pos/updates/stable/latest.json
    /pos/updates/beta/latest.json
    /pos            (the product page linking the latest setups: the app's "Télécharger" buttons open it)
    /pos/support    (the default support page: "Ouvrir la page d'assistance" in the help centre and Réglages › Diagnostic)

The files come from `dist-release\<version>\` after `pnpm release:build` and
`pnpm release:manifest`: the setups (x64, and x86 after `pnpm release:build --arch x86`) and the one
`SHA256SUMS.txt` listing them all at its top, the manifests in
`updates\stable\latest.json` and `updates\beta\latest.json` (a beta has only the `beta` one).

Order when publishing: the setups first, the manifests last (a manifest makes a release live).
Serve `latest.json` with `Cache-Control: no-cache` and the setups with a long cache. Check:

    Invoke-RestMethod https://dzbuild.com/pos/updates/stable/latest.json | Select-Object version, rollout, paused

To pull a release: `pnpm release:manifest --pause` for that version and upload the manifest.

Copies installed before 1.0.0 still ask the old endpoint (`https://dzbuild.dev/pos/updates/<channel>/latest.json`)
for their updates: keep serving a manifest there (or a permanent redirect to the `dzbuild.com` one) until
those copies have moved to 1.0.0.
