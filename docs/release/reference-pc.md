# The reference-PC benchmark (HW-047, NFR-001)

Every stable release is measured on the reference PC: Celeron J1900 (or a 3rd/4th-gen i3),
4 GB RAM, HDD, 1024×768 touch, Windows 10 22H2, the release installed, the Rust toolchain
present (`winget install Rustlang.Rustup`, with the Visual Studio Build Tools "Desktop
development with C++" that the README lists) and a checkout of the release's tag
(`git checkout v<version>`). Reboot the PC, then from the checkout:

    powershell -ExecutionPolicy Bypass -File scripts\perf\bench.ps1

It runs the release-mode budget tests of pos-core and pos-hw (the first run compiles them in
release, which takes a while on a J1900; the busy year alone took about 10 minutes on the
development PC, so allow an hour or more on the reference PC), starts
DZBuild POS five times and reads `cold_start_ms` from the log (the app's own mark, logged by
`app_ready` on the first login screen; run 1 is the first start after the reboot, the others
are warm), measures the idle memory with `idle-ram.ps1` once you have logged in and opened the
sale screen, and writes `docs\release\perf-<date>.md` with one line per budget (`OK` / `OVER`).
Between runs it closes the app as a person would (the backup on close may run): a killed app
would make the next start an unclean one, which runs the integrity check. Paste that file into
the release checklist, section 5. A line `OVER` blocks the release.

`-Exe <path>` measures a copy installed elsewhere (default
`C:\Program Files\DZBuild POS\dzbuild-pos.exe`); `-Runs <n>` changes the number of starts.
