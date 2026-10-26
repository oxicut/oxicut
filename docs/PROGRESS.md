# Progress log

One entry per phase (see `roadmap.md`).

## Phase 0: Environment setup (2026-10-26)

Toolchain installed and verified: MSVC Build Tools, Rust 1.99.1 (clippy, rustfmt), Node 24.22.1, WebView2 155, tauri-cli 2.12.4, uv 0.13.5, cargo-nextest, cargo-watch. ScanSplitter 0.8.0 cloned to `reference/ScanSplitter` and runnable via `uv run scansplitter`.

## Phase 0.5: UI shell with mock data (2026-10-26)

Reprioritised at the user's request: app and UI before the engine. Tauri 2 + Vue 3 + TypeScript in `app/` (crate `oxcut-app`, product name "OxCut"), with a Fabric.js review canvas, scan queue, settings, model manager and first-run dialog, all on sample data. Build the exe with `cd app; cargo tauri build --no-bundle`; output is `target\release\oxcut-app.exe`. Dev mode: `cd app; cargo tauri dev`.

Gotcha: Windows PowerShell 5.1 `Set-Content -Encoding utf8` writes a BOM, which Tauri's JSON config parser rejects. Use `[IO.File]::WriteAllText` with `UTF8Encoding($false)`. The matching `Get-Content -Raw` reads as the ANSI code page, so a read-modify-write round trip also turns non-ASCII characters into mojibake (`±` became `Ã‚Â±` in `roadmap.md`, repaired). Edit files with the Edit tool, or use `[IO.File]::ReadAllText` / `WriteAllText`.
