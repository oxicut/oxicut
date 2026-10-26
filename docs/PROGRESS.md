# Progress log

One entry per phase (see `roadmap.md`).

## Phase 0: Environment setup (2026-10-26)

Toolchain installed and verified: MSVC Build Tools, Rust 1.99.1 (clippy, rustfmt), Node 24.22.1, WebView2 155, tauri-cli 2.12.4, uv 0.13.5, cargo-nextest, cargo-watch. ScanSplitter 0.8.0 cloned to `reference/ScanSplitter` and runnable via `uv run scansplitter`.

## Rename: OxCut to OxiCut (2026-10-26)

"OxCut" read as "cutting an ox", so the project is now **OxiCut** ("oxide" + cut, the Rust convention). Crates are `oxicut-core`, `oxicut-models`, `oxicut-cli` and `oxicut-app`; the command is `oxicut`; the Tauri identifier is `app.oxicut`; models live in `%APPDATA%\oxicut\models` (override: `OXICUT_MODELS_DIR`). Names checked free on crates.io, GitHub, npm and PyPI (`oxicut`, `oxicut-core`, `oxicut-models`, `oxicut-cli`) on 2026-10-26; the `oxicut` GitHub org and the crates are not claimed yet. Earlier entries below keep describing things as they were then; the code and docs use the new name everywhere. The project folder on disk is still named `OxCut`.

## Phase 0.5: UI shell with mock data (2026-10-26)

Reprioritised at the user's request: app and UI before the engine. Tauri 2 + Vue 3 + TypeScript in `app/` (crate `oxicut-app`, product name "OxiCut"), with a Fabric.js review canvas, scan queue, settings, model manager and first-run dialog, all on sample data. Build the exe with `cd app; cargo tauri build --no-bundle`; output is `target\release\oxicut-app.exe`. Dev mode: `cd app; cargo tauri dev`.

Gotcha: Windows PowerShell 5.1 `Set-Content -Encoding utf8` writes a BOM, which Tauri's JSON config parser rejects. Use `[IO.File]::WriteAllText` with `UTF8Encoding($false)`. The matching `Get-Content -Raw` reads as the ANSI code page, so a read-modify-write round trip also turns non-ASCII characters into mojibake (`±` became `Ã‚Â±` in `roadmap.md`, repaired). Edit files with the Edit tool, or use `[IO.File]::ReadAllText` / `WriteAllText`.

## Phase 1: Workspace, v3 port, CLI (2026-10-26)

`oxicut-core` has the whole v3 detector as a Rust port of ScanSplitter 0.8.0, and `oxicut` has `detect` and `crop`. Every stage was checked against the Python original with golden data from `tools/dump_v3_reference.py` (`benchmarks/golden/v3.json`, 114 fixture scans, 320 boxes):

| Stage | Match with Python |
|---|---|
| Lab conversion | exact for L over all 2^24 colours; `a`/`b` exact except ~1,100 dark blues |
| Background clusters (k-means) | exact on 111 of 114 images |
| Foreground threshold | Otsu equal on all; pixel counts exact on 106 |
| Mask, contours, candidates | candidates exact on 112 of 114 |
| Merge, dedupe, gutter split | exact on all 114 |
| Angle snap (Gaussian, Canny, Hough) | exact on all 320 boxes |
| Side refinement | exact on all 320 boxes |
| End to end (identical pixels) | same box count on all 114; 111 within 2 px / 0.5 degrees |

The three outside tolerance, and why, are in `docs/detector-v7-port.md`. The biggest lessons from the port: ScanSplitter pins OpenCV 5, whose behaviour differs from OpenCV 4 in several places that matter (gray weights, linear warps in floating point, `minAreaRect` angle range, contour order); and Rust's JPEG decoder is not bit-identical to PIL's, so parity tests read PIL-decoded pixels.

Run the checks with `cargo nextest run` (the golden tests need `reference/ScanSplitter` and `reference/decoded`, made by `tools/dump_v3_reference.py`; they skip without them).
