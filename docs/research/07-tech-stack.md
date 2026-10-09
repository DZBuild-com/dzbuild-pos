# 07 — Technology Stack for DZBuild POS (offline-first Windows desktop POS / gestion commerciale)

> Research date: **2026-09-28**. Every version number and platform claim below links to a source.
> Anything I could not confirm from a source is marked **(unverified)**. Anything that is my own design choice is marked **(design decision)**.
> Scope: which desktop technology, database, sync model, hardware and printing layer, document/import/export libraries, UI kit, testing, update, signing and anti-tamper approach to use for a **free**, **installable**, **offline-first** Windows POS for Algerian shops. The product can optionally link to a DZBuild account (dzbuild.dev OAuth).

---

## 0. TL;DR: the recommendation

| Layer | Pick | Why (short) |
|---|---|---|
| Desktop shell | **Tauri 2.12+** (Rust core + WebView2), Windows 10/11 x64, with an x86 build as an option | ~3–10 MB installer. RAM is several times lower than Electron. Full Win32 access from Rust (raw printing, serial, spooler). The updater is signed. Chromium-grade Arabic/RTL rendering. The team already knows web tech. |
| UI | **React 19 + TypeScript + Vite 8 + Tailwind 4 + shadcn/ui (RTL mode, Jan 2026)** | shadcn/ui now has first-class RTL. React has the biggest ecosystem and the best tooling. Dense tables via **TanStack Table v9 + TanStack Virtual**. |
| State / forms / i18n | **TanStack Query** (IPC cache), **Zustand** (cart and UI state), **react-hook-form + zod**, **i18next/react-i18next** (`i18n.dir()`), native `Intl` for DZD formatting | Proven and typed. `Intl` already formats `ar-DZ` and `fr-DZ` correctly (verified below). |
| Rust ↔ TS contract | **tauri-specta** (2.0.0-rc.24) generated bindings | One typed contract. The UI never sends raw SQL. |
| Database | **SQLite 3.53.x** through **rusqlite 0.40** (`bundled-sqlcipher-vendored-openssl`, `backup`, `hooks`, `session`), **WAL + synchronous=FULL** for money tables, migrations with **rusqlite_migration**, optional **SQLCipher** at rest | Single file, zero admin, crash-safe, fast on HDD/Celeron. The UI does not use tauri-plugin-sql (so the webview has no SQL access). |
| Multi-terminal LAN | **Host/Client mode**: one PC runs the embedded **axum** HTTP + WebSocket server (inside the same app), discovered through **mDNS (mdns-sd)**. Each client keeps a **local SQLite replica and an outbox**, so a till keeps selling when the LAN or host is down. **Append-only ledgers** plus **UUIDv7** IDs mean merges never conflict. | Works with no internet at all. No third-party sync service. The CRDT/sync products are beta, cloud-bound or stalled (see §6). |
| Cloud link (optional) | The same outbox pushes to the **dzbuild.dev API** over HTTPS. Login uses **OAuth 2.0 + PKCE with a loopback redirect (RFC 8252)**. Tokens go in **Windows Credential Manager** through `keyring`. | Standards-based, no custom URL scheme to hijack. |
| Receipt printing | Our own ESC/POS layer (based on the **escpos** crate 0.20) sent **RAW through the Windows spooler (winspool)**, the USB printer class, TCP port 9100 or a COM port. **Arabic receipts print as raster images** (shaped by Chromium canvas or Rust text shaping, then dithered to 1-bit, then `GS v 0`). Latin text prints in native text mode. | Thermal printer firmware cannot shape Arabic. Raster output is the only reliable way. |
| A4 documents / PDF | HTML/CSS templates rendered in a hidden WebView2, then **`PrintToPdf`** or **silent `Print`** (`ICoreWebView2_16`) through Tauri's `with_webview`. Optional **Typst 0.15** (`typst-as-lib`) for headless PDFs. | Perfect Arabic shaping, users can edit the templates, and printing needs no dialog. |
| Excel / CSV | **calamine** (read xlsx/xls/ods) + **rust_xlsxwriter 0.95** (write) + **csv** crate, all in Rust | The npm `xlsx` package is stuck on 0.18.5 with known CVEs, and ExcelJS is effectively unmaintained. |
| Barcodes | Printer-native barcode commands for receipts. **bwip-js 4.11** (100+ symbologies, SVG/canvas) for labels and A4. **TSPL/ZPL** raw commands for label printers. | |
| Updates | **Tauri updater plugin** (mandatory signature, static JSON on dzbuild.dev/CDN, stable and beta channels). The **LAN host can mirror updates** to the tills. Updates install only at shift close. A pre-migration DB backup runs automatically. | Built in and signed. Installers are small, so delta updates (Velopack) are not needed. |
| Installer | **NSIS** `-setup.exe` with `embedBootstrapper` for WebView2 (+1.8 MB), plus a separate **offline full installer** (`offlineInstaller`, ~127 MB) for USB distribution. NSIS hooks add the firewall rule. | Works without internet. French/Arabic installer languages. |
| Signing | Sign every binary (Smart App Control blocks unsigned files). Use an **OV certificate on a cloud HSM** for direct downloads. Consider **Microsoft Store** as a second channel: registration is free and the Store signs, so SmartScreen never warns. **Azure Artifact Signing is not available to Algerian individuals or organisations** (§13). | |
| Logs / crashes | `tracing` plus rolling files (tauri-plugin-log for the frontend), **out-of-process minidumps** (`minidumper`, `sentry-rust-minidump`) stored locally and uploaded **only with consent**, and a one-click **diagnostic ZIP** for support over WhatsApp/email | Works offline and respects privacy. |
| Tests | **Vitest** (UI, with Tauri `mockIPC`), **cargo-nextest + insta + proptest** (core), **WebdriverIO `@wdio/tauri-service`** (officially recommended) or **Playwright over CDP** for E2E, a **virtual ESC/POS printer** for byte-level snapshots, **cargo-fuzz** on importers | |

**Main risks:** WebView2 memory on 4 GB PCs, Rust hiring and learning curve, the SmartScreen cold start for a new publisher, and the drop of Windows 7. These are covered in §18 with mitigations.

---

## 1. Constraints that drive the choice (Algerian market)

### 1.1 Windows versions actually in use in Algeria
StatCounter desktop Windows version share for Algeria, **August 2026** ([source](https://gs.statcounter.com/windows-version-market-share/desktop/algeria)):

| Version | Share |
|---|---|
| Windows 11 | 52.76 % |
| Windows 10 | 46.02 % |
| Windows 7 | 0.86 % |
| Windows 8.1 | 0.19 % |
| Windows 8 | 0.12 % |
| Windows XP | 0.04 % |

*Caveat:* StatCounter measures web page views, not installed PCs. Shop back-office PCs may skew older than this **(unverified)**. Even so, Windows 7/8/8.1 is about 1.2 %. Every modern framework has dropped those versions (see §2).

**WebView2 on Windows 10 is safe until at least October 2028.** Microsoft states: *"Microsoft Edge and the Microsoft WebView2 Runtime will continue to receive updates on Windows 10 22H2 until at least October 2028 … The ESU program won't be required"* ([Microsoft Learn: Edge supported OS](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-supported-operating-systems)). The same page says Edge/WebView2 support for Windows 7/8/8.1 **ended at version 109** (January 2023), and that CPUs without **SSE3** stopped receiving Edge updates at version 126–128. That rules out very old Pentium 4-class machines.

### 1.2 Hardware / environment assumptions (design targets)
- **Low-end PCs:** Celeron/Pentium or old i3, **4 GB RAM**, often an HDD, sometimes all-in-one touch POS terminals **(unverified; to be confirmed by the market research files 01–06)**. Budget: cold start under 3 s and total RAM under 250 MB including WebView2 child processes **(design decision)**.
- **Offline by default.** Unreliable internet. Power cuts are frequent in some regions **(unverified)**, so every committed sale must survive a sudden power loss. This drives `synchronous=FULL` (§5.2).
- **Bilingual UI:** French and Arabic (RTL). Arabic receipts are required. Digits: the Algerian locales use Latin digits. I checked this with the ICU/CLDR data in Node 24: `Intl.NumberFormat('ar-DZ',{style:'currency',currency:'DZD'})` gives `1.234.567,50 د.ج.‏` with numbering system `latn`, and `fr-DZ` gives `1 234 567,50 DA` (verified locally, 2026-09-28).
- **Peripherals:** 58/80 mm ESC/POS thermal printers (USB, LAN, serial), cash drawers (kicked by the printer), HID barcode scanners (keyboard wedge), label printers (TSPL/ZPL), weighing scales (serial, and EAN-13 barcodes with an embedded weight), customer pole displays (VFD/serial) or a second monitor.
- **Several tills per shop** on a LAN, plus a back-office PC.

---

## 2. Desktop framework comparison (late-2026 versions)

### 2.1 Current versions & OS floors (evidence)

| Framework | Current version (date) | Minimum Windows | Evidence |
|---|---|---|---|
| **Tauri 2** | **2.12** released 2026-09-26 (core `tauri` 2.11.5 on 2026-07-01) | **Windows 10+** from 2.12: *"Tauri decided it's time to drop Windows 7 support"*. The MSRV was raised to **Rust 1.90**. Rust itself requires Windows 10+ for `*-pc-windows-*` targets since 1.78. | [Tauri 2.12 blog](https://v2.tauri.app/blog/tauri-2.12/), [tauri changelog](https://v2.tauri.app/release/tauri/all-versions/), [Rust Win7 blog](https://blog.rust-lang.org/2024/02/26/Windows-7/) |
| **Electron** | **44.3.0** (2026-09-09), Chromium 152, Node 24 | **Windows 10+** since Electron 23. Electron 22 was the last with Win7/8/8.1. | [Electron 44](https://www.electronjs.org/blog/electron-44-0), [Farewell Win7/8/8.1](https://www.electronjs.org/blog/windows-7-to-8-1-deprecation-notice) |
| **.NET 10 (LTS)** + WPF / WinUI 3 / Avalonia / MAUI | .NET 10 (Nov 2025; servicing update 2026-07-14) | The official matrix lists Windows 11 and **only Enterprise/IoT/LTSC Windows 10** builds (1607 E, 1809 E, 21H2 E/IoT). Consumer Win10 22H2 is not on the list because it is out of support. *"There is no longer a version of .NET that's supported on Windows 7 and Windows 8.1"* (.NET 6 was the last). | [.NET 10 supported OS](https://github.com/dotnet/core/blob/main/release-notes/10.0/supported-os.md), [Install .NET on Windows](https://learn.microsoft.com/en-us/dotnet/core/install/windows) |
| **WinUI 3 / Windows App SDK** | 2.4.0 stable (2026-09-09) | Windows 10 1809+ | [Windows App SDK](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/), [WinUI 3](https://learn.microsoft.com/en-us/windows/apps/winui/winui3/) |
| **Avalonia** | **12.0** (2026-04-07); NuGet 12.1.3 | Not stated on the release page **(unverified)**; runs on .NET 8/10, so effectively Win10+ | [Avalonia 12 blog](https://avaloniaui.net/blog/avalonia-12), [NuGet](https://www.nuget.org/packages/avalonia) |
| **Flutter** | **3.47** (2026-08-12) | **Windows 10/11 supported. Windows 8 and earlier unsupported.** | [Flutter supported platforms](https://docs.flutter.dev/reference/supported-platforms) |
| **Qt** | **6.11** (2026-03-23); 6.10 supported until 2026-10-07 | Windows 10 1809+. **"Qt 6.12 will be the last version to support Windows 10."** | [Qt supported platforms](https://doc.qt.io/qt-6/supported-platforms.html), [Qt releases](https://doc.qt.io/qt-6/qt-releases.html) |
| Wails (Go + WebView2) | v3 **beta** (2026-08-02); v2 stable | Win10+ (WebView2) | [Wails v3 beta](https://v3.wails.io/blog/wails-v3-beta/) |

**Conclusion on Windows 7/8.1:** no maintained modern framework supports them. The only ways to support them would be pinning Tauri ≤ 2.11 with Rust 1.77 and WebView2 109 (frozen, insecure), or WPF on .NET Framework 4.8. Given the ~1.2 % share, **we target Windows 10 1809+ and Windows 11, x64** (plus an optional i686 build for old 32-bit Windows 10 PCs) **(design decision)**.

### 2.2 Size / RAM / startup (evidence and caveats)

| Metric | Tauri 2 | Electron | Source |
|---|---|---|---|
| Bundle/installer | ~3–10 MB; 8.6 MiB in one measured demo | 80–200 MB; 244 MiB in the same demo | [gethopp benchmark (macOS, N=1, Apr 2025)](https://www.gethopp.app/blog/tauri-vs-electron), [Rustify 2026](https://rustify.rs/articles/rust-tauri-vs-electron-2026), [dev.to/gethopp](https://dev.to/gethopp/tauri-vs-electron-performance-bundle-size-and-the-real-trade-offs-1el4) |
| RAM | ~30–80 MB idle (simple apps); ~172 MB with 6 windows | ~150–400 MB; ~409 MB with 6 windows | same |
| Startup | "consistently under 0.5 s" (claims); the gethopp test found the difference *negligible* | 1–2 s typical | same |
| Real-world warning | WebView2 renderer processes can grow large if the app leaks. One Tauri app reported **~914 MB** in a renderer. | Same Chromium behaviour | [terax-ai #238](https://github.com/crynta/terax-ai/issues/238), [hoppscotch #6340](https://github.com/hoppscotch/hoppscotch/issues/6340) |

The benchmarks are mostly third-party marketing-grade numbers. **We must measure our own build on a 4 GB Celeron reference PC** as an acceptance test (§16). WebView2 is shared with Edge on the system, so part of its memory is not "ours". The app can also set `MemoryUsageTargetLevel = LOW` when minimized or inactive ([ICoreWebView2_19](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2_19)).

### 2.3 Scored comparison for *this* product

Scores are 1–5 (5 = best), my assessment using the evidence above.

| Criterion (weight) | Tauri 2 + React | Electron + React | .NET 10 WPF | WinUI 3 | Avalonia 12 | Flutter 3.47 | Qt 6 (QML/C++) |
|---|---|---|---|---|---|---|---|
| Installer size (M) | **5** (3–10 MB) | 1 (80–200 MB) | 3 (framework-dependent small; self-contained large, **unverified** sizes) | 2 | 3 | 3 (**unverified** ~20–40 MB) | 3 |
| RAM on 4 GB PCs (H) | 4 | 2 | 4 | 3 | 4 | 4 | **5** |
| Startup (M) | 4 | 3 | 4 | 3 | 4 | 4 | 5 |
| Win10/11 support horizon (H) | 5 (WebView2 on Win10 until ≥ 2028) | 5 | 4 (.NET 10 lists only Enterprise/IoT Win10) | 4 | 4 | 5 | **2** (Qt 6.12 is the last with Win10) |
| Win7/8.1 (L) | ✗ (≥ 2.12) | ✗ | ✗ (.NET Framework 4.8 WPF ✓) | ✗ | ✗ | ✗ | ✗ |
| Dev speed for a web team (H) | 4 (Rust backend has a learning curve) | **5** | 3 | 2 | 3 | 3 | 2 |
| UI quality & dense POS UI (H) | **5** (web ecosystem: shadcn, TanStack) | **5** | 3 (dated unless themed) | 4 | 4 | 4 | 4 |
| Arabic RTL & bidi quality (H) | **5** (Chromium: HarfBuzz + ICU) | **5** | 5 ([WPF FlowDirection + number substitution](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/advanced/bidirectional-features-in-wpf-overview)) | 4 | 4 (RTL fixes in v12) | 4 | 4 |
| Raw printing / serial / Win32 (H) | **5** (Rust `windows` crate, escpos, serialport) | 3 (native Node addons such as serialport and printer bindings; must be rebuilt per Electron ABI) | **5** | 4 | 4 | 3 (plugins less mature on Windows, **unverified**) | 5 |
| Silent printing of HTML/A4 (M) | 4 (WebView2 `Print` via `with_webview`) | 4 (`webContents.print({silent,deviceName})`, [known bugs](https://github.com/electron/electron/issues/39092)) | 5 | 4 | 3 | 3 | 5 |
| SQLite access (H) | **5** (rusqlite + SQLCipher bundled) | 4 (better-sqlite3 native module) | 5 (Microsoft.Data.Sqlite) | 5 | 5 | 4 (drift/sqlite3 FFI) | 5 |
| Auto-update (H) | **5** (signed updater built in) | 4 (electron-updater / Velopack) | 4 (Velopack) | 3 (MSIX) | 4 (Velopack) | 3 | 3 |
| Anti-tamper / reverse-engineering cost (M) | 4 (compiled Rust; web assets embedded in the binary) | **1** (asar is trivially unpacked) | 2 (IL decompiles easily unless obfuscated) | 2 | 2 | 3 (AOT) | 4 |
| Licensing risk (M) | 5 (MIT/Apache) | 5 | 5 | 5 | 4 (core MIT; some components paid) | 5 | 3 (LGPL obligations or commercial licence) |
| Long-term bus factor (M) | 4 | 5 | 5 | 3 | 4 | 4 | 5 |
| **Weighted verdict** | **Best overall** | Runner-up if RAM and disk were not a concern | Best "Windows-only native" alternative | Not recommended | Credible alternative | Credible but thin desktop hardware ecosystem | Rejected: Win10 support ends at 6.12, plus licensing and dev speed |

### 2.4 Per-framework notes

**Tauri 2 (recommended).**
- WebView2 is Chromium-based and evergreen: *"you are guaranteed a relatively recent chromium build on all Windows targets"* ([Tauri webview versions](https://v2.tauri.app/reference/webview-versions/)).
- The Windows installer can embed the WebView2 bootstrapper (~1.8 MB) or the full offline installer (~127 MB), or pin a fixed version (~180 MB). Install scope can be perUser (no admin), perMachine or both ([Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/)).
- On Windows, `with_webview` exposes the native `ICoreWebView2Controller`, so any WebView2 API (silent Print, PrintToPdf, CapturePreview, MemoryUsageTargetLevel) is reachable from Rust ([Tauri commit exposing with_webview](https://github.com/tauri-apps/tauri/commit/c82b4761e1660592472dc55308ad69d9efc5855b), [Webview docs](https://docs.rs/tauri/latest/tauri/webview/struct.Webview.html)).
- Tauri 2.12 also added `on_permission_request` and explicit resource management ([blog](https://v2.tauri.app/blog/tauri-2.12/)).
- Risks: WebView2 memory, Rust learning curve, and small community plugins (bus factor). We write our own hardware crates and use the community plugins only as references.

**Electron.** The fastest for a pure-JS team. Silent printing is first-class (`webContents.print({ silent: true, deviceName })`), but it has long-standing issues on some printers and with the `silent` flag ([#39092](https://github.com/electron/electron/issues/39092), [#20651](https://github.com/electron/electron/issues/20651)). It fails our low-end-hardware and anti-tamper goals: installers of 100 MB and more, 150–400 MB RAM, and JS source readable from `app.asar`.

**.NET 10 + WPF.** The strongest native option. WPF bidi is excellent: `FlowDirection`, and `NumberSubstitution` for Arabic digits. Win32 printing and ESC/POS libraries are mature, and Velopack supports .NET. The downsides:
- The UI looks dated unless it is heavily themed.
- IL is easy to decompile.
- The .NET 10 support matrix officially lists only Enterprise/IoT Windows 10 editions.
- The web team would need a new skill set.
- It does not reuse DZBuild's web UI knowledge.
Pick this only if the team is C#-first.

**Avalonia 12.** MIT core. Big performance gains in v12: *"1,867% FPS gains in complex scenes"* and compiled bindings on by default. RTL mirroring fixes ([Avalonia 12](https://avaloniaui.net/blog/avalonia-12)). Some premium components (charts, PDF viewer, rich text) are subscription-based. It is a good native alternative, but the ecosystem for dense data grids with RTL is smaller than the web's.

**WinUI 3 / MAUI.** Windows App SDK 2.4 needs Win10 1809+. MAUI's Windows target is WinUI 3 and MAUI is mobile-first. Neither is recommended for a dense, keyboard-heavy POS.

**Flutter 3.47.** Win10/11 only ([supported platforms](https://docs.flutter.dev/reference/supported-platforms)). Good RTL (Directionality). The desktop plugin ecosystem for raw printing, serial and spooler access on Windows is thinner **(unverified depth)**. It would introduce Dart for a web team.

**Qt 6.** Excellent performance, but **"Qt 6.12 will be the last version to support Windows 10"** ([Qt supported platforms](https://doc.qt.io/qt-6/supported-platforms.html)). That is a real problem when 46 % of Algerian Windows is still Win10. There are also LGPL/commercial licensing obligations ([Qt licensing](https://doc.qt.io/qt-6/licensing.html)), and C++/QML is the slowest option for this team. Rejected.

**Context: incumbents.** Many French and Algerian "gestion commerciale / caisse" products are built with **WinDev + HFSQL** ([PC SOFT depot example](https://depot.pcsoft.fr/resource.awp?file_id=281474976711860%3Bprogiciel-gestion-caisse-commercants-windev-windev-mobile), [developpez forum](https://www.developpez.net/forums/d2152651/environnements-developpement/windev/windev-recherche-logiciel-module-gestion-commerciale/)). Those are the products we compete with, and a modern web-grade UI is a visible differentiator. That the Algerian products specifically use WinDev is **(unverified)**; see research files 01–06.

---

## 3. Final recommendation & justification

**Tauri 2.12+ with a Rust "core" workspace and a React 19 + TypeScript frontend.**

1. **Low-end hardware.** A ~10 MB installer and native Rust business logic; the webview only renders.
2. **Crash safety.** All writes go through one Rust writer thread, with SQLite in WAL mode and `synchronous=FULL` on money paths. The webview never holds authoritative state.
3. **Arabic + French.** Chromium's HarfBuzz/ICU gives best-in-class bidi. shadcn/ui ships RTL-aware components. `Intl` formats DZD correctly.
4. **Printing.** Rust reaches winspool RAW, USB, TCP and COM directly, and WebView2 gives silent HTML printing and PDF output.
5. **Multi-terminal.** The same binary can act as LAN host (axum) or client. The same Rust core runs in both, so business rules live in one place.
6. **Updates & integrity.** The updater requires signatures (it *cannot* be disabled). Assets are compiled into the binary. Rust code is costly to reverse-engineer.
7. **Team fit.** DZBuild is a web company. The UI is React; the Rust surface is kept small and well-typed through tauri-specta.

---

## 4. Architecture (bird's-eye)

```
┌──────────────────────────── DZBuild POS.exe (Tauri 2) ─────────────────────────────┐
│  WebView2 (React 19 UI, fr/ar, RTL)                                                 │
│   • POS screen, back-office, reports, settings, template designer                   │
│   • TanStack Query ⇄ typed IPC (tauri-specta)   • Zustand cart   • i18next          │
│        │ invoke()/events (JSON)                                                     │
│  ──────┼─────────────────────────────────────────────────────────────────────────── │
│  Rust  ▼                                                                             │
│  app-shell crate: Tauri commands (thin), windows (main, customer display), tray,     │
│                   updater, single-instance, deep-link/OAuth, log                     │
│  pos-core crate (pure Rust, no Tauri dep → testable & reusable by server):           │
│     domain (sales, returns, stock ledger, pricing, taxes, customers/credit, cash)    │
│     db (rusqlite, single writer thread + reader pool, migrations, backups)           │
│     sync (outbox/inbox, idempotency, LAN + cloud transport)                          │
│  pos-hw crate: escpos encoder, raster (Arabic), winspool RAW, USB, TCP 9100,         │
│                serial (scales, VFD), label TSPL/ZPL, cash drawer                     │
│  pos-docs crate: xlsx/csv import-export, PDF (WebView2 PrintToPdf / Typst)           │
│  pos-server crate (optional runtime role): axum HTTP+WS, mDNS advert, auth for tills │
└──────────────────────────────────────────────────────────────────────────────────────┘
      ▲ LAN (HTTP/WS, mDNS _dzpos._tcp)                    ▲ HTTPS (when online)
      │                                                    │
  Other tills (same exe, "client" role, local replica)   dzbuild.dev API (OAuth, cloud backup, sync)
```

Principles **(design decisions)**:
- **The frontend is stateless for money.** Cart state lives in Zustand for speed, but a sale is committed only by a Rust command in a single transaction that returns the final document.
- **No SQL in the webview.** Do not use `tauri-plugin-sql`, because it exposes SQL to JS ([plugin docs](https://v2.tauri.app/plugin/sql/)). Expose only domain commands, with Tauri capabilities kept to least privilege.
- **One core, three roles:** *standalone*, *host* and *client*, chosen by configuration.

---

## 5. Database layer

### 5.1 Engine & driver
- **SQLite 3.53.4** (2026-07-24) is current. 3.54.0 is expected next ([sqlite.org news](https://sqlite.org/news.html), [release log](https://sqlite.org/releaselog/current.html)).
- **rusqlite 0.40.2** has the features we need: `bundled`, `bundled-sqlcipher`, `bundled-sqlcipher-vendored-openssl`, `backup`, `session` (requires `hooks`), `hooks`, `preupdate_hook`, `blob`, `functions`, `vtab`, `serde_json`, `uuid`, `chrono`/`time`, `array`, `load_extension` ([docs.rs features](https://docs.rs/crate/rusqlite/latest/features)). Bundled SQLCipher now builds from the pinned `sqlcipher-src` crate and tracks SQLite 3.53.4 ([rusqlite fork PR](https://github.com/LucaCappelletti94/rusqlite/pull/10)).
- **Why rusqlite rather than sqlx or tauri-plugin-sql:**
  - It is synchronous, which fits a dedicated writer thread.
  - It gives first-class SQLCipher, backup API and session extension access.
  - tauri-plugin-sql (sqlx-based, v2.4.0) is designed for JS-side SQL ([docs](https://v2.tauri.app/plugin/sql/)), which we reject.
  - A rusqlite fork of the plugin exists with SQLCipher and transactions ([tauri-plugin-rusqlite2](https://github.com/razein97/tauri-plugin-rusqlite2)) and is useful as a reference only.
- **Migrations:** **rusqlite_migration 2.5.0** stores the version in `PRAGMA user_version`, needs no CLI and embeds SQL in the binary ([crate](https://crates.io/crates/rusqlite_migration), [README](https://github.com/cljoly/rusqlite_migration/blob/master/README.md)). *Alternative:* refinery, which keeps a history table ([refinery](https://github.com/rust-db/refinery)). Migrations are **forward-only**, and an automatic backup runs before any migration **(design decision)**.
- **Embedded PostgreSQL:** rejected. It adds hundreds of MB and a service to manage, gives no benefit for a single-shop dataset, and complicates backup/restore for non-technical users **(design decision; size unverified)**.

### 5.2 Durability settings (crash & power-loss)
From the [SQLite pragma docs](https://sqlite.org/pragma.html) and the [WAL docs](https://sqlite.org/wal.html):
- WAL + `synchronous=NORMAL` is atomic and consistent and survives *application* crashes, but **"a transaction committed in WAL mode with synchronous=NORMAL might roll back following a power loss"**. With `synchronous=FULL`, the WAL is synced at each commit ([agwa analysis](https://www.agwa.name/blog/post/sqlite_durability), [avi.im](https://avi.im/blag/2025/sqlite-fsync/)).
- **Our settings (design decision):**
  - `journal_mode=WAL`
  - `synchronous=FULL` (sales, payments and cash movements must survive power cuts; the cost is one fsync per sale, fine at POS rates)
  - `foreign_keys=ON`, `busy_timeout=5000`
  - `wal_autocheckpoint` left at the default, plus an explicit `wal_checkpoint(TRUNCATE)` at shift close
  - `temp_store=MEMORY`, a moderate `cache_size`
  - `PRAGMA quick_check` at startup, a full `integrity_check` weekly and before backups
- **A single writer thread** (channel-fed) with a small **reader pool**. This avoids `SQLITE_BUSY` storms and keeps transactions short.

### 5.3 Encryption at rest (optional, default ON for new installs)
- **SQLCipher** through rusqlite `bundled-sqlcipher-vendored-openssl` (no system OpenSSL on Windows).
- The key is generated per install and stored in **Windows Credential Manager** through the `keyring` crate 4.2.0 ([docs](https://docs.rs/keyring/latest/keyring/)), which is DPAPI-backed per user. For multi-user shops, a machine-scope DPAPI blob is an alternative **(design decision)**.
- *Honest limit:* the key lives on the same machine, so this stops casual tampering (for example, editing prices or deleting sales with "DB Browser for SQLite"). It does not stop a determined local admin. The audit hash chain (§11) covers detection.

### 5.4 Schema principles (design decisions)
- **Money:** `INTEGER` centimes (i64). Never floats. Quantities are `INTEGER` milli-units (grams for weighed goods).
- **IDs:** **UUIDv7** as primary keys (time-ordered, generated offline on any till). Human document numbers are per series and per till (for example `T02-2026-000123`) to avoid collisions.
- **Append-only ledgers:** `stock_movements`, `cash_movements`, `customer_ledger`, `supplier_ledger`. Balances are derived or cached and can be rebuilt. This makes LAN/cloud merges conflict-free (§6).
- **Immutable fiscal documents:** a validated sale or invoice is never updated, only reversed through a credit note or return. Each row carries `prev_hash` and `hash` (a SHA-256 chain) for tamper evidence (§11).
- **Time:** timestamps stored in UTC. The business date is computed in `Africa/Algiers` (UTC+1, no DST **(unverified; confirm with tz database)**).
- **Full-text search** for products through the SQLite FTS5 virtual table, matching French and Arabic names, barcodes and references.

### 5.5 Backups (design decisions based on SQLite primitives)
- **Mechanisms:** the online **backup API** (rusqlite `backup` feature: fewer CPU cycles, incremental) or **`VACUUM INTO`**. VACUUM INTO is *"an alternative to the backup API for generating backup copies of a live database"*, produces a minimal file, and purges deleted content ([SQLite VACUUM](https://sqlite.org/lang_vacuum.html)). Never copy the `.db` file raw while the app is running.
- **Schedule:**
  - On every shift close (Z report)
  - Hourly while the app is open (keep 24)
  - Daily (keep 30), monthly (keep 12)
  - Before every update or migration
- **Targets:**
  1. Local `%LOCALAPPDATA%\DZBuild POS\backups`
  2. **USB stick:** detect removable drives (Win32 `GetDriveTypeW`) and offer a one-click "Sauvegarder sur clé USB"
  3. **LAN host**, which pulls from the tills
  4. **DZBuild cloud** (optional, when linked): client-side encrypted (`age` or AES-GCM) before upload
- **Verification:** every backup is re-opened read-only and passes `PRAGMA integrity_check` before it counts as good. The restore wizard lists backups with date, size and sales count.

---

## 6. Multi-terminal LAN & sync

### 6.1 Options evaluated

| Option | Status (evidence) | Fit for offline Algerian LAN |
|---|---|---|
| **cr-sqlite** (CRDT SQLite extension) | Last release **v0.16.3 on 2025-01-17**, nothing since ([releases](https://github.com/vlcn-io/cr-sqlite/releases)); npm last published about 2 years ago ([npm](https://www.npmjs.com/package/@vlcn.io/crsqlite)) | ✗ Development appears stalled. Too risky for money data. |
| **ElectricSQL** 1.0 (GA 2025-03-17) | A **read-path** sync engine for **Postgres**: *"Electric does not do write-path sync"* ([writes guide](https://electric-sql.com/docs/guides/writes), [landscape](https://www.localfirst.fm/landscape/electricsql)) | ✗ Needs Postgres and a server. Writes need our own API. Not LAN-first. |
| **PowerSync** | The **Tauri SDK is alpha**: *"the underlying Rust SDK and the IPC protocol … are subject to change"* ([docs](https://docs.powersync.com/client-sdks/reference/tauri), [announcement](https://releases.powersync.com/announcements/introducing-the-powersync-tauri-sdk-alpha)). The backend must be Postgres, MongoDB, MySQL or SQL Server plus the PowerSync Service ([service repo](https://github.com/powersync-ja/powersync-service)); the licence of the self-hosted Open Edition is **(unverified)**. Writes go through an upload queue to *your* backend. | ◐ Could be a good choice for **cloud** sync later, but not as a LAN solution now. |
| **Turso / libSQL sync** | Offline Sync was a **public beta**, "not yet recommended for production", with conflict *detection* only ([blog 2025-03](https://turso.tech/blog/turso-offline-sync-public-beta)). **Turso Sync** (2025-10-08) is still beta: Last-Push-Wins, **Turso Cloud only** ([blog](https://turso.tech/blog/introducing-databases-anywhere-with-turso-sync)). | ✗ Cloud-bound and beta. No LAN host. |
| **SQLite Session extension** (changesets/patchsets) | Built into SQLite and exposed by rusqlite (`session` feature). It records changes and applies them to another DB with the same schema ([SQLite sessions](https://sqlite.org/sessionintro.html), [rusqlite features](https://docs.rs/crate/rusqlite/latest/features)) | ◐ A useful *transport format* for replicating master data (catalogue, prices) from host to clients. |
| **Custom outbox/inbox over HTTP/WS (host-authoritative)** | Plain engineering, built from our own ledger design | ✅ **Recommended** |

### 6.2 Recommended LAN design (design decision)
- **Roles:**
  - *Standalone:* a single PC.
  - *Host ("Poste principal"):* runs an axum server on a fixed port, advertises `_dzpos._tcp` through **mdns-sd**. mdns-sd is pure Rust, runs its daemon thread over flume channels, is still labelled beta, and has been tested with Avahi and Bonjour ([mdns-sd](https://github.com/keepsimple1/mdns-sd), [docs](https://docs.rs/mdns-sd/latest/mdns_sd/)). A manual IP entry is the fallback.
  - *Client ("Caisse"):* pairs with the host once through a QR or PIN code.
- **Data ownership:**
  - **Master data** (products, prices, customers, users, settings) is **host-authoritative**. Clients hold a **read replica**, updated by delta pulls (`since` cursor), by session changesets, or pushed over WebSocket.
  - **Transactions** (sales, returns, cash movements, stock movements) are **created locally on the till** with UUIDv7 IDs and per-till numbering. They are written to the local DB *and* to an **outbox**, then pushed to the host with **idempotency keys**. The host acknowledges each one and the entry is marked as sent.
  - **Stock** is the sum of movements from all tills, so pushes merge without conflict. Negative-stock policy is enforced softly (a warning), as shops expect.
  - **Conflicts** are only possible on master-data edits made on a client while it is offline. They resolve as *host-wins with a field-level audit*; or client edits of master data can simply require the host to be online.
- **If the host is offline, tills keep selling** from their replica and the outbox drains when the host returns. This is the key resilience property for Algerian conditions.
- **Security on the LAN:** a pairing token per till (random 256-bit), sent as a bearer token over HTTP. Optional self-signed TLS pinned at pairing **(design decision)**.
- **Windows Firewall:** the NSIS `NSIS_HOOK_POSTINSTALL` hook ([Tauri NSIS hooks](https://v2.tauri.app/distribute/windows-installer/)) adds an inbound rule for the app on the *Private* profile.
- **Cloud (DZBuild) sync:** the same outbox protocol, sent to `api.dzbuild.dev` when linked and online. The server side can later adopt PowerSync or ElectricSQL behind our API without changing the client contract.

---

## 7. Hardware & printing

### 7.1 Transports for thermal printers
| Transport | How in Rust | Notes |
|---|---|---|
| **Windows spooler RAW** (most common: the printer has a Windows driver, e.g. "XP-80C") | `windows` crate: `OpenPrinterW` → `StartDocPrinterW` (datatype `"RAW"`) → `WritePrinter` → `EndDocPrinter` | The same approach as [tauri-plugin-thermal-printer](https://github.com/luis3132/tauri-plugin-thermal-printer) (EnumPrintersW/OpenPrinterW/WritePrinter). Works with any installed driver. |
| **USB printer class (usbprint.sys)** | `escpos` crate `usbprint` feature: *"drives POS printers through the standard Windows usbprint.sys kernel driver using … CreateFile / ReadFile / WriteFile"* ([lib.rs](https://lib.rs/crates/escpos)) | Allows **status reads** (paper-out, drawer open). |
| **Network (TCP 9100)** | `escpos` `NetworkDriver` | For LAN or Wi-Fi printers and kitchen printers. |
| **Serial (COM)** | `escpos` `serial_port` feature, or `serialport` 4.x | Older printers and VFD customer displays. |
| **Native USB (libusb)** | `escpos` `usb`/`native_usb` | Only if needed; it can conflict with the Windows driver. |

- **escpos crate 0.20.0** (2026-09-02, MIT) has features `barcodes` (UPC/EAN/CODE39/ITF/CODABAR), `codes_2d` (QR, PDF417, DataMatrix, Aztec), `graphics` (raster), `usb`, `native_usb`, `hidapi`, `serial_port`, `usbprint`, `ui`, `full`. Drivers: Network, SerialPort, Usb/NativeUsb, **WindowsDriver**, File, Console ([docs.rs](https://docs.rs/escpos)).
- Alternatives and references: [tauri-plugin-esc-pos](https://github.com/Kosadchiy/tauri-plugin-esc-pos) (USB+TCP) and [tauri-plugin-thermal-printer](https://github.com/luis3132/tauri-plugin-thermal-printer) (MIT, text/image/barcode/QR/table/cut/drawer, claims Arabic support). **We wrap escpos in our own `pos-hw` crate** so a small community plugin is never a single point of failure **(design decision)**.
- **Cash drawer:** the ESC/POS pulse command (`ESC p m t1 t2`) through the printer's RJ11 port. Trigger it on cash payments, and from a manual button (logged in the audit trail).
- **Paper widths:** 58 mm (384 dots) and 80 mm (576 dots) at 203 dpi **(common printer spec; confirm per model)**.

### 7.2 Arabic on thermal printers: print as raster
- ESC/POS firmware *"can basically print the Arabic characters that the code page provides, but things like right-to-left support and glyph shaping is simply not supported … Most programmers give up with Arabic and send a raster image"* ([EscPosEncoder #4](https://github.com/NielsLeenheer/EscPosEncoder/issues/4); see also [escpos-php RTL #6](https://github.com/mike42/escpos-php/issues/6), [escpos-coffee #44](https://github.com/anastaciocintra/escpos-coffee/issues/44), [CP864](https://en.wikipedia.org/wiki/Code_page_864), [Epson code tables](https://download4.epson.biz/sec_pubs/pos/reference_en/charcode/index.html)).
- **Pipeline (design decision):**
  1. Build the receipt layout model: lines, columns, logo, QR.
  2. Render at exactly 384 or 576 px width. **Primary:** an **OffscreenCanvas** in WebView2 (Chromium shapes Arabic through HarfBuzz; fonts are bundled locally). **Headless fallback on the LAN host:** Rust text shaping with **HarfRust**, the official HarfBuzz port. HarfRust *"matches HarfBuzz v14.3.1"*, and RustyBuzz is now archived with users told to switch ([harfrust](https://github.com/harfbuzz/harfrust), [lib.rs](https://lib.rs/crates/harfrust), [rustybuzz](https://github.com/harfbuzz/rustybuzz)). Use it with `unicode-bidi` and `tiny-skia` for rasterising.
  3. Threshold or dither to 1-bit, band into 24-line or `GS v 0` chunks, and send.
  4. Pure-Latin receipts can use **text mode** (faster, sharper). The user chooses per printer: "Texte" or "Image (arabe)".
- **Fonts** (all bundled, SIL OFL): Noto Naskh/Sans Arabic, Cairo, IBM Plex Sans Arabic or Tajawal for Arabic; Inter or Noto Sans for Latin **(licences are well-known OFL; confirm each file's licence when bundling)**.

### 7.3 A4 documents & silent printing
- WebView2 offers `ShowPrintUI`, **`Print` (silent, programmatic settings including printer name, copies, page ranges, media size, margins, headers/footers, duplex, colour)**, **`PrintToPdf`** and **`PrintToPdfStream`** ([Microsoft Learn: printing from WebView2](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/print), [ICoreWebView2PrintSettings2](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2printsettings2?view=webview2-1.0.3351.48)).
  - Caveats: only one print job runs at a time, and some printer names with full-width characters have failed ([WebView2Feedback #3379](https://github.com/MicrosoftEdge/WebView2Feedback/issues/3379)).
- **Plan:** a hidden "print" webview loads the document template (HTML + CSS `@page`, `dir="rtl"` for Arabic). Rust then calls `ICoreWebView2_16::Print` for silent printing or `ICoreWebView2_7::PrintToPdf` for PDF export and email, both through `with_webview`.
- **Documents:** facture, facture proforma, bon de livraison, bon de commande, bon de réception, avoir, devis, état de stock, relevé client, Z/X reports. The contents come from research files 01–06.

### 7.4 PDF alternatives
- **Typst 0.15.1** (2026-07-17) ([releases](https://github.com/typst/typst/releases/)). It can be embedded with **typst-as-lib**, which bakes templates, fonts and packages into the binary with *"zero Typst-related runtime dependencies"* ([crate](https://crates.io/crates/typst-as-lib)). RTL works by setting `lang`/`dir` (*"setting the dominant direction gives the bidirectional reordering algorithm the necessary information"*) ([Typst text docs](https://typst.app/docs/reference/text/text/)); the auto-bidi package helps ([auto-bidi](https://typst.app/universe/package/auto-bidi/)). Use it for **headless PDF generation on the LAN host** or in batch jobs (monthly statements) **(design decision)**. It adds binary size **(unverified MB)**.
- react-pdf / pdfmake: not needed. They have weaker Arabic shaping than Chromium **(unverified)**.

### 7.5 Other peripherals
- **Barcode scanners (HID keyboard wedge):**
  - A global key buffer detects scanner bursts (inter-key interval under about 30 ms, terminated by Enter), so scans work even when the focus is not in the search box **(design decision)**.
  - Support GS1 **weighted/priced EAN-13 with prefixes 20–29** (restricted circulation, embeds the weight or price, used by label scales) **(GS1 convention; confirm the prefix layout per scale brand)**.
- **Serial devices** (scales, VFD pole displays, serial scanners): `serialport` 4.x. `tauri-plugin-serialplugin` depends on `serialport ^4.9` ([docs.rs](https://docs.rs/crate/tauri-plugin-serialplugin/latest), [serialport](https://docs.rs/serialport)). Scale protocols (CAS, Dibal, Mettler, etc.) go in a **driver registry** configured from the UI **(protocol specifics unverified)**.
- **Customer display:** a second Tauri window on a second monitor (multi-window is native), or a VFD over serial with the ESC/POS display command set.
- **Label printers** (Xprinter, TSC, Zebra, Godex): generate **TSPL/EPL/ZPL** and send it RAW through the spooler. Include a label designer with price, barcode, name in Arabic/French and a shelf-label layout.
- **Barcode generation:** **bwip-js 4.11.4**, *"encoding modules for over 100 different barcode types"*, SVG or canvas output ([npm](https://www.npmjs.com/package/bwip-js), [site](https://bwip-js.metafloor.com/)). In Rust: `qrcode` ([docs](https://docs.rs/qrcode)) and `barcoders` (v2.0.0, last updated about 2 years ago) ([crates.io keyword](https://crates.io/keywords/barcode)). `rxing` 0.8.x can *decode* (camera or image) if ever needed ([lib.rs](https://lib.rs/crates/rxing)). Receipts use the printer's native barcode and QR commands.

---

## 8. Import / Export (Excel, CSV, PDF, JSON)

| Need | Library | Evidence |
|---|---|---|
| Read `.xlsx/.xlsm/.xls/.ods` | **calamine** (pure Rust, read-only, lazy per-sheet parsing) | [calamine docs](https://docs.rs/calamine/latest/calamine/), [comparison vs umya-spreadsheet (2026-05)](https://umaranis.com/2026/05/04/reading-excel-files-in-rust-calamine-vs-umya-spreadsheet/) |
| Write `.xlsx` (styles, formats, formulas, autofilter, freeze panes, RTL sheets) | **rust_xlsxwriter 0.95.0** (2026-05-09), by the author of Python XlsxWriter | [docs.rs](https://docs.rs/crate/rust_xlsxwriter/latest), [GitHub](https://github.com/jmcnamara/rust_xlsxwriter) |
| CSV (`;` separator, UTF-8 BOM for Excel-FR, Windows-1256 / CP1252 legacy imports) | `csv` crate + `encoding_rs` **(design decision)** | |
| JS-side Excel | **Avoid.** The npm `xlsx` is stuck at **0.18.5** with known vulnerabilities; current SheetJS (0.20.3) ships only from cdn.sheetjs.com. **ExcelJS** is considered *inactive*. | [SheetJS CDN](https://cdn.sheetjs.com/xlsx/), [npm xlsx](https://www.npmjs.com/package/xlsx), [ExcelJS maintenance discussion](https://github.com/exceljs/exceljs/discussions/2884), [pkgpulse 2026](https://www.pkgpulse.com/guides/sheetjs-vs-exceljs-vs-node-xlsx-excel-files-node-2026) |
| PDF | WebView2 `PrintToPdf` (primary) and Typst (headless) | §7.3–7.4 |

**Import UX (design decision):**
- Column-mapping wizard with auto-detection of FR/AR headers.
- Dry-run preview with row-level errors, then commit in a single transaction.
- Templates to download for products, customers, suppliers, opening stock, prices and barcodes.
- Importers are **fuzz-tested** because they parse untrusted files.

**Export:** every table view can export to Excel, CSV and PDF, and bulk exports cover the accountant's needs. The exact accounting formats and G50 declarations belong to the market research files.

---

## 9. Frontend stack (inside WebView2)

| Concern | Choice | Evidence / notes |
|---|---|---|
| Build | **Vite 8** (Rolldown bundler) + `@vitejs/plugin-react` v6 (Oxc, no Babel) | [Vite 8 announcement](https://vite.dev/blog/announcing-vite8) |
| UI library | **React 19** + TypeScript | Largest ecosystem. Svelte 5 or Solid would give smaller bundles, but shadcn RTL, TanStack and team skills favour React **(design decision)**. |
| Styling | **Tailwind CSS 4** with **logical properties** (`ms-*`, `pe-*`, `text-start`) | Matches shadcn RTL. |
| Components | **shadcn/ui** with `rtl: true`: the CLI converts physical classes to logical ones and flips icons (`rtl:rotate-180`). New projects use `init --rtl`; existing ones use `migrate rtl`. Introduced **January 2026**. | [shadcn RTL changelog](https://ui.shadcn.com/docs/changelog/2026-01-rtl), [RTL docs](https://ui.shadcn.com/docs/rtl), [Direction component](https://ui.shadcn.com/docs/components/radix/direction) |
| Alternative kit | **Mantine 8** (all components RTL; `DirectionProvider` follows the `<html dir>` mutation since 8.3) | [Mantine RTL](https://mantine.dev/styles/rtl/), [8.3.0](https://mantine.dev/changelog/8-3-0/) |
| Data grids | **TanStack Table v9** (stable 2026-08-04; tree-shakable, TanStack Store state, lower memory) + **TanStack Virtual** for 10k+ row lists | [Table v9 blog](https://tanstack.com/blog/announcing-tanstack-table-v9), [InfoQ](https://www.infoq.com/news/2026/07/tanstack-table-v9-beta/) |
| Server-state | **TanStack Query** wrapping typed IPC calls (cache and invalidate on Rust events) | |
| Client-state | **Zustand** (cart, UI, hotkey mode) | |
| Forms | **react-hook-form + zod**; the same schemas generate TS types, and Rust re-validates | |
| i18n | **i18next + react-i18next**. `i18n.dir()` returns `rtl` for `ar` and sets `<html dir lang>`. French is the default, with Arabic and English. | [i18next dir guide](https://phrase.com/blog/posts/localizing-react-apps-with-i18next/), [RTL with Radix](https://dev.to/omerrkosar/beyond-translation-mastering-rtl-layouts-with-i18next-and-radix-ui-3159) |
| Formatting | Native `Intl` (`ar-DZ` and `fr-DZ` verified above) | |
| Charts | shadcn charts (Recharts) or Apache ECharts for dashboards **(design decision)**. Mirror axes in RTL. | |
| Command palette & hotkeys | `cmdk` palette (Ctrl+K). A central hotkey map (F1–F12 like legacy POS: F2 search, F4 qty, F8 discount, F9 pay, F12 validate) that users can remap. Full keyboard operation. A **touch mode** with large targets for all-in-one terminals. | **(design decision)** |
| Typed IPC | **tauri-specta 2.0.0-rc.24** (typed commands and events, `bindings.ts`) | [tauri-specta](https://github.com/specta-rs/tauri-specta), [specta docs](https://specta.dev/docs/tauri-specta) |
| Fonts | Bundled locally (no CDN; offline) | |

Performance rules **(design decision)**:
- Code-split the back-office from the POS screen.
- Keep the POS route's JS under about 300 kB gzipped.
- Virtualise every list.
- Never keep full tables in memory; page through Rust queries instead.
- Set WebView2 memory to LOW when minimized.

---

## 10. Tauri plugins & Rust crates (bill of materials)

**Official Tauri v2 plugins to use:**
- `single-instance` (*"must be the first one to be registered"*) ([docs](https://v2.tauri.app/plugin/single-instance/))
- `updater` ([docs](https://v2.tauri.app/plugin/updater/))
- `log` ([docs](https://v2.tauri.app/plugin/logging/))
- `deep-link` (for `dzpos://` activation, optional)
- `dialog`, `fs` (scoped), `opener`, `process`
- `autostart` (host PC), `window-state`, `notification`, `os`

Plugins we **do not** use: `sql` (§4). `stronghold`: the docs still list it without a deprecation notice ([docs](https://v2.tauri.app/plugin/stronghold/)), but OS-backed `keyring` is simpler for our needs **(design decision)**.

**Community references (study, don't depend on):**
- `tauri-plugin-oauth` (localhost redirect server) ([lib.rs](https://lib.rs/crates/tauri-plugin-oauth))
- `tauri-plugin-thermal-printer`, `tauri-plugin-esc-pos`
- `tauri-plugin-serialplugin`
- `sentry-tauri` ([crate](https://crates.io/crates/sentry-tauri))

**Core crates:**
- Async and server: `tokio`, `axum`, `tower-http`, `reqwest` (rustls)
- Serialisation, errors and IDs: `serde`/`serde_json`, `thiserror`, `uuid` (v7)
- Database: `rusqlite` (+ `rusqlite_migration`)
- Logging: `tracing`, `tracing-subscriber`, `tracing-appender`
- Hardware: `escpos`, `image` (dither), `windows` (winspool, drives, DPAPI), `serialport`, `mdns-sd`
- Documents: `calamine`, `rust_xlsxwriter`, `csv`, `encoding_rs`, optional `typst-as-lib`
- Text and raster: `harfrust` + `unicode-bidi` + `tiny-skia` (headless raster)
- Security and packaging: `keyring`, `sha2`, `zip`, `age` (backup encryption)
- Crash handling: `minidumper`/`crash-handler` or `sentry-rust-minidump`
- Bindings: `specta` + `tauri-specta`
- Optional: `windows-service` **(names are well-known crates; versions to pin at scaffold time)**

---

## 11. Security, integrity & anti-tamper (for a free product)

The product is free, so "licensing" is about **integrity, trust and account linking**, not copy protection.

1. **Signed binaries and signed updates.** Authenticode signatures on the EXE and installer. The Tauri updater verifies signatures and *"cannot be disabled"* ([updater](https://v2.tauri.app/plugin/updater/)). Keep the private key in CI secrets and back it up offline: *"if you lose this key you will NOT be able to publish new updates"*.
2. **Tamper-evident sales journal.** Each fiscal document and each Z report stores `hash = SHA-256(prev_hash ‖ canonical_row)`. A "Vérifier l'intégrité" tool re-walks the chain and flags gaps. Deletions are not allowed; returns are reversals. This protects the shop owner from cashier fraud, which is the realistic threat **(design decision)**.
3. **Roles & permissions:** cashier, supervisor, manager, admin. Sensitive actions (void, discount above threshold, price override, drawer open, stock adjustment, delete customer credit) require a PIN or manager override, and every one is recorded in the **audit log**.
4. **At-rest encryption:** SQLCipher, with the key in Credential Manager (§5.3).
5. **Webview hardening:**
   - Strict **CSP**, no remote content, no `eval`.
   - **Capabilities** limited to the commands each window needs. The customer-display window gets read-only events.
   - Consider Tauri's **Isolation pattern**: all IPC goes through a sandboxed iframe and is AES-GCM encrypted. It protects against malicious npm dependencies. Caveat: on Windows, *"ES Modules won't successfully load"* in the isolation iframe ([isolation](https://v2.tauri.app/concept/inter-process-communication/isolation/)).
6. **Binary hardening:** release profile with `lto = "fat"`, `codegen-units = 1`, `strip = true`. Frontend assets are embedded in the executable. There is no Node runtime to unpack (unlike Electron's asar).
7. **DZBuild link:** OAuth tokens in Credential Manager, a refresh-token rotation policy, and device registration. Server-side flags (for example, a revoked device) are honoured when online, and the app never locks sales offline **(design decision)**.
8. **Supply chain:**
   - `cargo deny` and `cargo audit`, `npm audit` / `pnpm audit`, and lockfiles committed.
   - Renovate/Dependabot.
   - Reproducible CI builds on `windows-latest`.

---

## 12. DZBuild account linking (OAuth)

- **Flow:** OAuth 2.0 Authorization Code + **PKCE (S256)** with a **loopback redirect** `http://127.0.0.1:{ephemeral}/callback`, following **RFC 8252** ([RFC 8252](https://www.rfc-editor.org/rfc/rfc8252.html)).
  - RFC 8252 §7.3: the authorisation server *"MUST allow any port"* for loopback redirects.
  - Loopback is preferred over custom schemes, which other local apps can claim ([example of a scheme-hijack issue](https://github.com/calimero-network/tauri-app/issues/260)).
  - The open-source `tauri-plugin-oauth` shows the temporary-localhost-server pattern ([lib.rs](https://lib.rs/crates/tauri-plugin-oauth)). We can implement it directly with axum in about 100 lines **(design decision)**.
- **dzbuild.dev server requirements:** register a public (no-secret) client, allow `http://127.0.0.1` with any port, require PKCE, and issue scopes such as `pos:sync`, `pos:backup` and `store:catalog` (to publish POS products to a DZBuild store).
- **Offline:** linking is optional. All POS features work unlinked. Linked features are cloud backup, multi-shop dashboard, catalogue sync with the DZBuild website/e-shop, and remote reports.

---

## 13. Distribution, installer, code signing & updates

### 13.1 Installer
- **NSIS** `-setup.exe` (Tauri). It supports `installMode` (perUser/perMachine/both), `languages` (French, Arabic, English), `NSIS_HOOK_PREINSTALL`/`POSTINSTALL` hooks, a custom template, and `minimumWebview2Version` ([Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/)).
- **Two artifacts (design decision):**
  1. `DZBuildPOS-x.y.z-setup.exe` with `embedBootstrapper` (~+1.8 MB). WebView2 is present on Windows 11 and on *"the vast majority of Windows 10 devices"*, but not all ([Microsoft: evergreen vs fixed](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/evergreen-vs-fixed-version), [distribution](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)).
  2. `DZBuildPOS-x.y.z-offline-setup.exe` with **`offlineInstaller`** (~127 MB), for shops with no internet and for USB distribution by resellers.
- **Install scope:** perMachine by default, because the firewall rule and multi-user shop PCs need it. Offer perUser as a fallback **(design decision)**.
- **Silent install** with `/S` (needed by the Microsoft Store and by resellers) ([Tauri MS Store](https://v2.tauri.app/distribute/microsoft-store/)).

### 13.2 Code signing realities (2026)
- **Unsigned is not an option:** *"On Windows 11 devices, the Smart App Control feature … will block execution of unsigned files unless the file has a positive reputation"* ([Microsoft Learn: SmartScreen reputation](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)).
- **EV no longer bypasses SmartScreen** (a March 2024 change). EV and OV build reputation the same way ([Microsoft](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation), [ToDesktop PSA](https://www.todesktop.com/blog/posts/windows-apps-psa-ev-certs-do-not-grant-immediate-reputation-anymore)). Buy **OV**, not EV.
- **Maximum certificate validity is 460 days** for certificates issued from **2026-03-01** (CA/B Forum ballot CSC-31) ([GlobalSign](https://www.globalsign.com/en/company/news-events/news/businesses-must-prepare-two-significant-certificate-lifecycle-reductions-march-2026), [DigiCert](https://www.digicert.com/blog/understanding-the-new-code-signing-certificate-validity-change)). Plan a yearly renewal and keep the same subject name so reputation carries over.
- **Azure Artifact Signing** (formerly Trusted Signing), from $9.99/month, is Microsoft's recommended non-Store route. But **individuals must be in the US/Canada**, and organisations must be in the US, Canada, EU, UK, Australia, NZ, Japan, South Korea, Singapore, Switzerland, Norway or Israel ([search summary of MS docs](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart), [Q&A: country not available](https://learn.microsoft.com/en-nz/answers/questions/5810735/cant-create-a-new-trusted-signing-individual-ident)). **An Algerian DZBuild entity is not eligible** unless it has a legal entity in an eligible country. It also requires a paid Azure subscription ([FAQ](https://learn.microsoft.com/en-us/azure/artifact-signing/faq)).
- **Practical path (design decision):**
  - **(a)** An OV certificate from a public CA that validates Algerian organisations, with the key in a **cloud HSM / remote signing service** so CI can sign. Tauri supports a custom `signCommand` for any tool, as well as Azure Key Vault through `relic` ([Tauri Windows signing](https://v2.tauri.app/distribute/sign/windows/)). Which CAs serve Algeria is **(unverified)**; research before purchase.
  - **(b)** Also publish on the **Microsoft Store**. Individual *and* company registration are now **free** ([Windows blog 2025-09](https://blogs.windows.com/windowsdeveloper/2025/09/10/free-developer-registration-for-individual-developers-on-microsoft-store/), [2026-05 company](https://blogs.windows.com/windowsdeveloper/2026/05/07/publish-to-microsoft-store-as-a-company-now-with-free-registration-and-faster-onboarding/)), in "nearly 200 markets" (Algeria's inclusion is **unverified**). *"Store-distributed apps are signed by a Microsoft certificate and are never subject to SmartScreen download warnings"*. Caveat: a Tauri EXE/MSI submitted to the Store must itself be code-signed, and must use the WebView2 offline installer mode ([Tauri MS Store](https://v2.tauri.app/distribute/microsoft-store/)). Only MSIX packages are re-signed by Microsoft; MSIX packaging of a Tauri app needs evaluation **(unverified effort)**.
- **Timestamp** every signature (RFC 3161) so signatures stay valid after the certificate expires.

### 13.3 Auto-update
| Option | Pros | Cons |
|---|---|---|
| **Tauri updater** (chosen) | Built in. Mandatory signature. Static JSON or dynamic server (`{{current_version}}`, `{{target}}`, `{{arch}}`, can roll back through a custom comparator). Windows modes `passive`/`basicUi`/`quiet`. | No delta updates. On Windows the app must exit during install ([docs](https://v2.tauri.app/plugin/updater/)). |
| **Velopack** (MIT, Rust core) | Delta packages, per-user install without admin, channels, ~2 s updates, Squirrel migration, SDKs for Rust/C#/JS ([GitHub](https://github.com/velopack/velopack), [velopack.io](https://velopack.io/), [docs.rs](https://docs.rs/velopack)) | A second packaging toolchain next to Tauri's bundler. Deltas matter little for a ~10 MB app. |
| MSIX / Store | OS-managed updates | Store-only channel; MSIX constraints |

**Update policy (design decision):**
- Check at startup and every 6 h when online.
- **Never install during an open shift.** Offer it at *clôture de caisse* or at the next launch.
- Take an automatic DB backup, then run migrations.
- Keep a stable and a beta channel through different endpoints.
- Stage the rollout (for example 10 % → 50 % → 100 %) with the dynamic server.
- **LAN mirror:** the host downloads the update once and serves it to the tills. The endpoint is overridden at runtime to `http://host:port/updates/...` and the signatures are still verified.
- A **kill switch** in the update manifest can pause a bad release.

---

## 14. Logging, crash reporting & diagnostics (offline-first)

- **Logs:** Rust `tracing` with daily rolling files (`tracing-appender`), plus **tauri-plugin-log** for frontend logs (`RotationStrategy::KeepAll`/`KeepOne`, `max_file_size`, `TargetKind::LogDir`). On Windows v2 the log dir is under `%LOCALAPPDATA%` ([logging docs](https://v2.tauri.app/plugin/logging/), [discussion](https://github.com/tauri-apps/tauri/discussions/9532)). Keep 14 days. Personal data (customer phone numbers) is redacted.
- **Native crashes:** an out-of-process handler. `sentry-rust-minidump` *"starts the current executable again … in crash reporter mode"* and writes the minidump ([lib.rs](https://lib.rs/crates/sentry-rust-minidump), [Embark crash-reporting write-up](https://jake-shadle.github.io/crash-reporting/)). **Store minidumps locally**, then upload **only after explicit user consent** when online. The server can be a self-hosted Sentry or GlitchTip **(design decision)**.
- **JS errors:** an error boundary per route, forwarded to the Rust log.
- **Diagnostic bundle:** Aide → "Exporter un diagnostic" creates a ZIP with the last logs, app/OS/WebView2 versions, DB schema version and integrity-check result, printer list and settings. It contains **no DB contents** unless the user ticks the box. It can be shared by WhatsApp or email.
- **Watchdog:** on the host role, if the axum server panics it restarts with backoff. Tauri commands return `Result` and never panic across FFI.

---

## 15. Testing & quality strategy

| Layer | Tooling | Notes |
|---|---|---|
| Rust domain | `cargo nextest`, **proptest** (money/tax/rounding invariants, stock ledger sums), **insta** snapshots (receipt layout, ESC/POS byte streams, XLSX structure) | The pure `pos-core` crate has no Tauri dependency, so tests are fast. |
| DB & migrations | Run every migration against **fixture DBs from every past version**. Crash tests kill the process in the middle of a transaction and verify `integrity_check`. | |
| Importers | **cargo-fuzz** on the CSV/XLSX parsers, plus golden files (FR/AR headers, CP1256) | |
| Printing | A **virtual printer** (escpos `FileDriver`/`ConsoleDriver`) captures bytes. A raster comparison checks Arabic receipts against golden PNGs. There is also a **hardware lab** with real Xprinter/Epson units (manual smoke test per release). | |
| Frontend unit/components | **Vitest** + Testing Library, with **`@tauri-apps/api/mocks` (`mockIPC`)** to fake commands | |
| E2E | Official: **WebdriverIO `@wdio/tauri-service`** (Windows/Linux/macOS; IPC mocking, log capture), or `tauri-driver` with msedgedriver on Windows ([Tauri WebDriver](https://v2.tauri.app/develop/tests/webdriver/)). Alternative: **Playwright `connectOverCDP`** to WebView2 via `--remote-debugging-port` (Windows-only) ([Playwright WebView2](https://playwright.dev/docs/webview2), [tauri-cdp template](https://github.com/Haprog/tauri-cdp)) | Enable CDP only in test builds. |
| RTL/i18n | Visual snapshots in `fr` and `ar`. Lint rule banning physical CSS classes (`ml-`, `left-`). Pseudo-localisation. | |
| Performance | Criterion benchmarks (search 50k products under 30 ms, sale commit under 20 ms on HDD) **(targets = design decisions)**. Memory and startup measured on the reference low-end PC in CI (self-hosted runner) or manually per release. | |
| Security | `cargo deny`/`audit`, `npm audit`, CSP tests, capability review checklist | |

---

## 16. Performance budget (design decisions, to verify on reference hardware)

| Metric | Target | How |
|---|---|---|
| Installer (online) | ≤ 15 MB | Tauri + embedBootstrapper |
| Cold start to usable POS screen | ≤ 3 s on Celeron/4 GB/HDD | Small initial JS chunk, lazy back-office, SQLite open under 50 ms |
| Idle RAM (all processes) | ≤ 250 MB | Single main window, virtualised lists, WebView2 `MemoryUsageTargetLevel=LOW` when minimized |
| Scan → line added | ≤ 50 ms | FTS5/indexed barcode lookup in Rust, optimistic UI |
| Sale commit incl. fsync | ≤ 30 ms (SSD), ≤ 80 ms (HDD) | Single transaction, `synchronous=FULL` |
| Receipt to paper | ≤ 1.5 s (Arabic raster) | Pre-rendered header/logo bitmap cache |
| DB size | 5 years of a busy shop under 2 GB **(unverified estimate)** | Integer money, normalised lines |

---

## 17. Windows service vs desktop app

- **Default:** the **app itself** hosts the LAN server when in *host* role. It lives in the system tray, starts at login through the `autostart` plugin, and closing the window minimises it to the tray. The single-instance plugin prevents duplicates.
- **Optional later:** a **Windows service** "DZBuild POS Server" using the `windows-service` crate, for back-office PCs where nobody logs in. It would reuse `pos-server` + `pos-core` unchanged, and its data would move to `%ProgramData%` **(design decision)**.
- Printing stays in the user session (spooler access and fonts). Services should not print A4 through a webview.

---

## 18. Risk register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | WebView2 RAM on 4 GB PCs | Med | High | Memory budget tests, virtualisation, MemoryUsageTargetLevel, a single window, no heavy chart libraries on the POS screen, leak checks in E2E |
| R2 | A shop PC lacks WebView2 or has it broken | Low–Med | High | embedBootstrapper plus an offline installer variant; a startup check with a repair link; `minimumWebview2Version` |
| R3 | Windows 7/8.1 users excluded | Low (~1.2 %) | Low–Med | Clear system requirements on dzbuild.dev; reseller guidance to upgrade to Win10 IoT/LTSC or Win11 |
| R4 | SmartScreen warnings for a new publisher; Artifact Signing unavailable in DZ | High (first months) | Med | OV certificate on a cloud HSM, Microsoft Store channel, installation video, consistent signing identity |
| R5 | Rust skills gap | Med | Med | Keep the Rust surface domain-focused, typed bindings, up-to-date library docs, strong test suite |
| R6 | Arabic thermal printing quality varies by printer | Med | Med | Raster pipeline, per-printer calibration (darkness, width, text vs image mode), hardware lab |
| R7 | Data loss on power cut or disk failure | Med | Very High | WAL + FULL, integrity checks, automatic multi-target backups with verification, USB/cloud copies |
| R8 | LAN host down during peak hours | Med | High | Tills sell from their local replica and outbox; host election is a manual "promote this till to host" |
| R9 | Community Tauri plugins abandoned | Med | Low | Own `pos-hw` crate; plugins used only as references |
| R10 | Losing the updater private key | Low | Very High | Offline encrypted backups of the key (two locations), documented rotation plan |
| R11 | WebView2 print API regressions | Low | Med | Fallback to PDF plus a SumatraPDF-style print path, or raw spooler for thermal printers; pin tested WebView2 minimum |
| R12 | Tauri breaking changes (v3) | Low (near term) | Med | Keep business logic in Tauri-free crates, so the shell is thin and replaceable |

---

## 19. Version pin table (as of 2026-09-28)

| Component | Version | Source |
|---|---|---|
| Tauri | 2.12 (2026-09-26); MSRV Rust 1.90 | [blog](https://v2.tauri.app/blog/tauri-2.12/) |
| Electron (reference) | 44.3.0 (2026-09-09) | [electronjs.org](https://www.electronjs.org/docs/latest/tutorial/electron-timelines) |
| .NET (reference) | 10 LTS | [supported OS](https://github.com/dotnet/core/blob/main/release-notes/10.0/supported-os.md) |
| Avalonia (reference) | 12.1.3 | [NuGet](https://www.nuget.org/packages/avalonia) |
| Flutter (reference) | 3.47 | [docs](https://docs.flutter.dev/reference/supported-platforms) |
| Qt (reference) | 6.11 | [Qt](https://doc.qt.io/qt-6/qt-releases.html) |
| Windows App SDK (reference) | 2.4.0 | [MS Learn](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/) |
| SQLite | 3.53.4 (2026-07-24) | [sqlite.org](https://sqlite.org/releaselog/current.html) |
| rusqlite | 0.40.2 | [docs.rs](https://docs.rs/crate/rusqlite/latest/features) |
| rusqlite_migration | 2.5.0 | [docs.rs](https://docs.rs/crate/rusqlite_migration/latest) |
| escpos | 0.20.0 (2026-09-02) | [docs.rs](https://docs.rs/escpos) |
| rust_xlsxwriter | 0.95.0 (2026-05-09) | [docs.rs](https://docs.rs/crate/rust_xlsxwriter/latest) |
| calamine | latest at scaffold | [docs.rs](https://docs.rs/calamine/latest/calamine/) |
| Typst | 0.15.1 (2026-07-17) | [GitHub](https://github.com/typst/typst/releases/tag/v0.15.1) |
| keyring | 4.2.0 | [docs.rs](https://docs.rs/keyring/latest/keyring/) |
| tauri-specta | 2.0.0-rc.24 | [search summary](https://github.com/specta-rs/tauri-specta/releases) |
| Vite | 8.x | [vite.dev](https://vite.dev/blog/announcing-vite8) |
| TanStack Table | v9 (stable 2026-08-04) | [blog](https://tanstack.com/blog/announcing-tanstack-table-v9) |
| Mantine (alt) | 8.3+ | [changelog](https://mantine.dev/changelog/8-3-0/) |
| bwip-js | 4.11.4 | [npm](https://www.npmjs.com/package/bwip-js) |
| WebView2 SDK docs | 1.0.4191.47 reference set | [MS Learn](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2_19) |
| Velopack (alt) | 1.2.x NuGet line | [NuGet](https://www.nuget.org/packages/Velopack/) |

React 19.x, TypeScript, Tailwind 4, TanStack Query/Router/Virtual, Zustand, react-hook-form, zod and i18next: pin the latest stable at scaffold time. Their exact current patch versions were not confirmed **(unverified exact versions)**.

---

## 20. Library docs to keep at hand

Libraries whose **current docs** should be checked while building, because their APIs changed recently:
- Tauri v2 (core, plugins: updater, log, single-instance, deep-link, autostart; capabilities; `with_webview`)
- rusqlite (+ SQLCipher features), rusqlite_migration
- axum / tokio
- escpos, serialport, mdns-sd, windows-rs (winspool, DPAPI)
- calamine, rust_xlsxwriter, typst-as-lib, harfrust
- tauri-specta / specta
- React 19, Vite 8, Tailwind 4, shadcn/ui (RTL), TanStack Table v9 / Query / Router / Virtual, Zustand, react-hook-form, zod, i18next/react-i18next, bwip-js
- Vitest, WebdriverIO `@wdio/tauri-service`, Playwright (WebView2)
- Microsoft WebView2 printing APIs

Project rules to encode in the contributor guide and CI **(design decisions)**:
- No floats for money.
- No SQL in the frontend.
- Only logical CSS properties.
- Every table export offers xlsx, csv and pdf.
- Every command returns `Result`.
- Migrations are forward-only, with an automatic backup.
- Every user-facing string goes through i18n in fr and ar.

---

## 21. Sources (all accessed 2026-09-28)

**Frameworks & platforms**
- Tauri 2.12 announcement: https://v2.tauri.app/blog/tauri-2.12/
- Tauri changelog: https://v2.tauri.app/release/tauri/all-versions/
- Tauri webview versions: https://v2.tauri.app/reference/webview-versions/
- Tauri Windows installer: https://v2.tauri.app/distribute/windows-installer/
- Tauri Windows signing: https://v2.tauri.app/distribute/sign/windows/
- Tauri Microsoft Store: https://v2.tauri.app/distribute/microsoft-store/
- Tauri updater: https://v2.tauri.app/plugin/updater/
- Tauri SQL plugin: https://v2.tauri.app/plugin/sql/
- Tauri logging: https://v2.tauri.app/plugin/logging/
- Tauri single instance: https://v2.tauri.app/plugin/single-instance/
- Tauri isolation: https://v2.tauri.app/concept/inter-process-communication/isolation/
- Tauri stronghold: https://v2.tauri.app/plugin/stronghold/
- Tauri WebDriver: https://v2.tauri.app/develop/tests/webdriver/
- Tauri with_webview commit: https://github.com/tauri-apps/tauri/commit/c82b4761e1660592472dc55308ad69d9efc5855b
- Rust drops Win7 (1.78): https://blog.rust-lang.org/2024/02/26/Windows-7/
- Electron 44: https://www.electronjs.org/blog/electron-44-0
- Electron timelines: https://www.electronjs.org/docs/latest/tutorial/electron-timelines
- Electron Win7/8.1 deprecation: https://www.electronjs.org/blog/windows-7-to-8-1-deprecation-notice
- Electron silent print issues: https://github.com/electron/electron/issues/39092 and https://github.com/electron/electron/issues/20651
- .NET 10 supported OS: https://github.com/dotnet/core/blob/main/release-notes/10.0/supported-os.md
- Install .NET on Windows: https://learn.microsoft.com/en-us/dotnet/core/install/windows
- WPF bidirectional features: https://learn.microsoft.com/en-us/dotnet/desktop/wpf/advanced/bidirectional-features-in-wpf-overview
- Avalonia 12: https://avaloniaui.net/blog/avalonia-12
- Avalonia NuGet: https://www.nuget.org/packages/avalonia
- Windows App SDK: https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/
- WinUI 3: https://learn.microsoft.com/en-us/windows/apps/winui/winui3/
- Flutter supported platforms: https://docs.flutter.dev/reference/supported-platforms
- Qt supported platforms: https://doc.qt.io/qt-6/supported-platforms.html
- Qt licensing: https://doc.qt.io/qt-6/licensing.html
- Wails v3 beta: https://v3.wails.io/blog/wails-v3-beta/
- Benchmarks: https://www.gethopp.app/blog/tauri-vs-electron, https://rustify.rs/articles/rust-tauri-vs-electron-2026, https://dev.to/gethopp/tauri-vs-electron-performance-bundle-size-and-the-real-trade-offs-1el4
- WebView2 memory reports: https://github.com/crynta/terax-ai/issues/238, https://github.com/hoppscotch/hoppscotch/issues/6340
- Windows share in Algeria: https://gs.statcounter.com/windows-version-market-share/desktop/algeria
- Edge/WebView2 supported OS (Win10 until ≥ Oct 2028): https://learn.microsoft.com/en-us/deployedge/microsoft-edge-supported-operating-systems
- WebView2 evergreen vs fixed: https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/evergreen-vs-fixed-version

**WebView2 APIs**
- Printing: https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/print
- PrintSettings2: https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2printsettings2?view=webview2-1.0.3351.48
- MemoryUsageTargetLevel: https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2_19
- PrintAsync bug: https://github.com/MicrosoftEdge/WebView2Feedback/issues/3379
- Playwright WebView2: https://playwright.dev/docs/webview2
- tauri-cdp template: https://github.com/Haprog/tauri-cdp

**Database & sync**
- SQLite news: https://sqlite.org/news.html
- SQLite current release: https://sqlite.org/releaselog/current.html
- Pragmas: https://sqlite.org/pragma.html
- WAL: https://sqlite.org/wal.html
- VACUUM INTO: https://sqlite.org/lang_vacuum.html
- Sessions: https://sqlite.org/sessionintro.html
- Durability analyses: https://www.agwa.name/blog/post/sqlite_durability, https://avi.im/blag/2025/sqlite-fsync/
- rusqlite features: https://docs.rs/crate/rusqlite/latest/features
- rusqlite SQLCipher PR: https://github.com/LucaCappelletti94/rusqlite/pull/10
- rusqlite_migration: https://github.com/cljoly/rusqlite_migration
- refinery: https://github.com/rust-db/refinery
- tauri-plugin-rusqlite2: https://github.com/razein97/tauri-plugin-rusqlite2
- cr-sqlite releases: https://github.com/vlcn-io/cr-sqlite/releases
- ElectricSQL writes: https://electric-sql.com/docs/guides/writes
- PowerSync Tauri SDK: https://docs.powersync.com/client-sdks/reference/tauri
- PowerSync service: https://github.com/powersync-ja/powersync-service
- Turso offline sync beta: https://turso.tech/blog/turso-offline-sync-public-beta
- Turso Sync: https://turso.tech/blog/introducing-databases-anywhere-with-turso-sync
- mdns-sd: https://github.com/keepsimple1/mdns-sd

**Printing, hardware, documents**
- escpos crate: https://docs.rs/escpos and https://lib.rs/crates/escpos
- tauri-plugin-thermal-printer: https://github.com/luis3132/tauri-plugin-thermal-printer
- tauri-plugin-esc-pos: https://github.com/Kosadchiy/tauri-plugin-esc-pos
- Arabic ESC/POS limitations: https://github.com/NielsLeenheer/EscPosEncoder/issues/4, https://github.com/mike42/escpos-php/issues/6, https://github.com/anastaciocintra/escpos-coffee/issues/44
- Epson code tables: https://download4.epson.biz/sec_pubs/pos/reference_en/charcode/index.html
- HarfRust: https://github.com/harfbuzz/harfrust
- rustybuzz (archived): https://github.com/harfbuzz/rustybuzz
- serialport: https://docs.rs/serialport
- tauri-plugin-serialplugin: https://docs.rs/crate/tauri-plugin-serialplugin/latest
- Typst 0.15.1: https://github.com/typst/typst/releases/tag/v0.15.1
- Typst text/RTL: https://typst.app/docs/reference/text/text/
- typst-as-lib: https://crates.io/crates/typst-as-lib
- auto-bidi: https://typst.app/universe/package/auto-bidi/
- calamine: https://docs.rs/calamine/latest/calamine/
- rust_xlsxwriter: https://github.com/jmcnamara/rust_xlsxwriter
- calamine vs umya: https://umaranis.com/2026/05/04/reading-excel-files-in-rust-calamine-vs-umya-spreadsheet/
- SheetJS CDN: https://cdn.sheetjs.com/xlsx/
- ExcelJS maintenance: https://github.com/exceljs/exceljs/discussions/2884
- bwip-js: https://www.npmjs.com/package/bwip-js
- qrcode: https://docs.rs/qrcode
- rxing: https://lib.rs/crates/rxing

**Frontend**
- Vite 8: https://vite.dev/blog/announcing-vite8
- shadcn RTL: https://ui.shadcn.com/docs/changelog/2026-01-rtl and https://ui.shadcn.com/docs/rtl
- Mantine RTL: https://mantine.dev/styles/rtl/
- TanStack Table v9: https://tanstack.com/blog/announcing-tanstack-table-v9
- tauri-specta: https://github.com/specta-rs/tauri-specta
- i18next RTL guides: https://phrase.com/blog/posts/localizing-react-apps-with-i18next/, https://dev.to/omerrkosar/beyond-translation-mastering-rtl-layouts-with-i18next-and-radix-ui-3159

**Distribution, signing, security**
- SmartScreen reputation: https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation
- EV PSA: https://www.todesktop.com/blog/posts/windows-apps-psa-ev-certs-do-not-grant-immediate-reputation-anymore
- 460-day certificates: https://www.globalsign.com/en/company/news-events/news/businesses-must-prepare-two-significant-certificate-lifecycle-reductions-march-2026, https://www.digicert.com/blog/understanding-the-new-code-signing-certificate-validity-change
- Artifact Signing: https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart, https://learn.microsoft.com/en-us/azure/artifact-signing/faq
- Artifact Signing country Q&A: https://learn.microsoft.com/en-nz/answers/questions/5810735/cant-create-a-new-trusted-signing-individual-ident
- Microsoft Store free registration: https://blogs.windows.com/windowsdeveloper/2025/09/10/free-developer-registration-for-individual-developers-on-microsoft-store/ and https://blogs.windows.com/windowsdeveloper/2026/05/07/publish-to-microsoft-store-as-a-company-now-with-free-registration-and-faster-onboarding/
- Velopack: https://github.com/velopack/velopack
- RFC 8252: https://www.rfc-editor.org/rfc/rfc8252.html
- tauri-plugin-oauth: https://lib.rs/crates/tauri-plugin-oauth
- keyring: https://docs.rs/keyring/latest/keyring/
- sentry-rust-minidump: https://lib.rs/crates/sentry-rust-minidump
- Crash reporting in Rust: https://jake-shadle.github.io/crash-reporting/
- sentry-tauri: https://crates.io/crates/sentry-tauri

**Market context**
- WinDev caisse examples: https://depot.pcsoft.fr/resource.awp?file_id=281474976711860%3Bprogiciel-gestion-caisse-commercants-windev-windev-mobile, https://www.developpez.net/forums/d2152651/environnements-developpement/windev/windev-recherche-logiciel-module-gestion-commerciale/
