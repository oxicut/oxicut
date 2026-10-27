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

## Phase 2: MobileSAM and the full v7 detector (2026-10-26)

`oxicut models status|download|verify|path` fetches the pinned MobileSAM pair (28.2 MB encoder, 16.5 MB decoder) through the checksum-verified downloader. `oxicut-core` runs both through `ort` and has the whole of ScanSplitter's v7, selectable as `--mode v7` in the CLI and as "v7 · most accurate" in the app once the models are downloaded.

Checked against Python with golden data (`tools/dump_sam_reference.py`, `tools/dump_v7_full_reference.py`; `benchmarks/golden/sam.json`, `v7_full.json`):

| Stage | Match with Python |
|---|---|
| MobileSAM encoder embedding | sum within 2e-5, first values within 3e-5 |
| Decoder masks (predicted IoU, pixel count) | IoU equal to 4 digits; pixel counts equal or off by at most 3 |
| v4 refined rectangles | IoU 1.0000 |
| Texture islands, closed frames, capped v3 | identical on all 114 fixtures |
| Chroma proposals | identical on 113 of 114 |
| v3 proposals, v5 set, anchors | identical on 112 of 114 (v3 edge cases from Phase 1) |
| Chosen after arbitration, final | identical on 113 of 114 |
| Scores / sharpness / ghost stats | within 2.2 / 0.035 / 1.2 on 687 aligned candidates (remap rounding; no effect on decisions beyond the one differing scan) |

Benchmark at a 0 px trim (`oxicut bench --mode v7`):

| Suite | Box quality | F1@0.85 | Count accuracy |
|---|---:|---:|---:|
| robust-dev | 83.6% | 77.6% | 74.5% |
| robust-holdout | 88.2% | 81.2% | 72.3% |

ScanSplitter publishes 88.8% box quality for v7 on the holdout set; v3 at the same trim gets 79.0%. On the hand-test scans v7 found the prints v7-lite missed: 5 instead of 4 on `scansplitter-03` and on the 16-bit Epson TIFF. A whole album page (`album-07`) is still one box, as in ScanSplitter.

Practical notes: `ort` 2.0.0-rc.15 has one `Environment` per process and sessions need `&mut` to run, so each worker thread keeps its own `MobileSam` (`sam::with_thread_sam`); the app uses two workers for v7. ONNX Runtime is linked statically, so the exe needs no extra DLL. v7 takes about 3 s per scan on this machine.

## Phase 2 close-out (2026-10-27)

Checked on a clean models folder (`OXICUT_MODELS_DIR` pointing at an empty folder):

| Check | Result |
|---|---|
| `oxicut models download`, then `verify` | both MobileSAM files fetched, checksums pass, `detect --mode v7` loads them (5 prints on `scansplitter-03`) |
| One byte flipped in the encoder, decoder cut to 5 MB | `verify` names both DAMAGED and exits 1; `download` deletes and re-fetches both; `verify` passes |
| No network (dead proxy, empty folder) | clear error, no `.part` file left, `--mode v3` still works, `--mode v7` says to run `models download` |

One fix came out of it: download errors only said "error sending request", hiding why. They now carry the cause ("connection refused", and so on). The Detection tab also warns when v7 is selected but the models are missing.

White-lid check on public stand-ins (the user's own scans were not at hand; none of these files are in the repo): three borderless photos on white (`3ladies1-3`, tilted 0 to 10 degrees), a white-bed scan of four cream-bordered prints (cropped from a VueScan screenshot), and a cream album page with four white-bordered prints. v3, v7-lite and v7 all find the right counts; on the cream-bordered prints the box sits on the photograph and not on the border. ScanSplitter v7 finds 4, 3, 3, 3 and 4 on the same files, so ours is not worse. Caveats: the VueScan crop has blue selection boxes drawn on it and the album page has green ones, and neither is a white lid. Still to do on the user's own Epson scans.
