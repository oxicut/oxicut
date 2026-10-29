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

White-lid check on public stand-ins (the user's own scans were not at hand; none of these files are in the repo): three borderless photos on white (`3ladies1-3`, tilted 0 to 10 degrees), a white-bed scan of four cream-bordered prints (cropped from a VueScan screenshot), and a cream album page with four white-bordered prints. v3, v7-lite and v7 all find the right counts; on the cream-bordered prints the box sits on the photograph and not on the border. ScanSplitter v7 finds 3, 3, 3, 4 and 3 on the same files (we find 3, 3, 3, 4 and 4: it misses one print on the album page), so ours is not worse. Caveats: the VueScan crop has blue selection boxes drawn on it and the album page has green ones, and neither is a white lid. Still to do on the user's own Epson scans.

## Phase 3: batch shell, first run, metadata (2026-10-27)

Most of Phase 3 came with the manual-testing milestone. What was left, and is now done:

- **First run**: the welcome dialog offers the MobileSAM download with its size and a progress percentage; when it finishes the detector switches to v7. v7-lite works in the meantime, also with no network (checked with a dead proxy: clear error, nothing half-written).
- **50 scans**: 50 scans on v7 (34 MB in total) added at start-up. The window was never reported "not responding" over 282 samples, and detection finished in about 45 s with two workers (peak memory about 1.2 GB, back to 57 MB afterwards).
- **DPI, colour profile and EXIF** (`oxicut-core::metadata`, pure Rust: `img-parts`, `kamadak-exif`, and the `tiff` crate that `image` already uses). The scan's resolution (JFIF density, EXIF, PNG `pHYs`, TIFF resolution tags), ICC profile and, from JPEG and PNG scans, EXIF are written into the crops of every output format. The EXIF is rewritten rather than copied: the scan's thumbnail and pixel-size fields are dropped. Round trips are tested per format, 16-bit TIFF pixels stay identical, and a 600 dpi test scan gave 600 dpi crops as JPEG, PNG and TIFF. A TIFF scan's EXIF is not carried over.
- **Software field**: a separate option (on by default; `--no-software-tag` on the CLI) writes `OxiCut 0.0.1` into the Software field: EXIF in JPEG and PNG, a `Software` text chunk in PNG, the baseline tag in TIFF. With it off, the scanner's own Software value stays when the metadata is kept; with both off, no EXIF is written at all.
- **Version**: `oxicut_core::VERSION`, hardcoded to `0.0.1` until the build supplies one. It is the value in the Software field, in `oxicut --version`, and at the bottom right of the app window.
- Download errors now include the cause, and the Detection tab warns when v7 is selected but its models are missing.

Left for you to check by hand: that the exported files open in Windows Photos and IrfanView.

## Phase 4: review mode (closed out 2026-10-28)

The review editor was built across the hand-testing rounds, so the checklist had fallen behind. Everything on it exists and is used daily: the per-scan Fabric.js canvas with rotatable, resizable boxes; add, delete, split, straighten and re-detect; zoom and pan; Shift snaps rotation to 90 degrees; Enter approves and moves on, the arrows step through the list, Delete removes a box; the five warnings (nothing found, touches the scan edge, unusual shape, much smaller than the rest, overlapping) and auto-approve of unflagged scans. Added beyond the list: loupes (one at a dragged handle, four while moving or rotating), per-scan undo and redo, collapsible scan list and panel with live status, a settings dialog with a light, dark or follow-system theme, and a neutral palette.

## Phase 5, step 1: projects that save themselves (2026-10-28)

A project is one `.oxicut` file, JSON with a format version, kept in the OxiCut folder in Documents (or anywhere: any `.oxicut` opens). It holds the scan list with boxes, statuses, the keys that tell edited from untouched and exported from not, and each scan's path both absolute and relative to the project file, so a project folder copied to another PC finds its scans. It holds no pixels: previews and thumbnails are rebuilt from the scans and cached, so the file stays small.

- **Autosave** one second after any change (a temporary file renamed over the old one, so a crash never leaves half a project) and flushed on closing. The creation time survives saves.
- **Reopening** restores everything; scans that were still being detected are detected again; scans whose file is gone are kept with their boxes, shown as "file not found", and skipped by export. The last project opens on start; a first run makes "My scans".
- **Projects dialog** from the project name in the top bar: open, new, rename, show in folder, remove from the list, open a file; a recent list with scan and approved counts. Switching waits while detection or export runs.
- **Single instance**: starting OxiCut again (double-clicking another project) hands the file to the window that is already open instead of starting a second copy. The file association itself is set in `tauri.conf.json` and applied by the installer (Phase 7); `tools/register-file-association.ps1` does it per user for a built copy.
- **Closing guard**: since work is saved, closing only asks while an export is running.
- Tested: save and load round trip, atomic write, damaged, newer-format and byte-order-mark files, relative-path recovery, unique names, rename, start-up argument splitting, thumbnail cache rebuild; end to end by killing the app and reopening, and by launching a second copy with a project path.

Next in Phase 5: front/back pairing, auto-pair, manifest.
