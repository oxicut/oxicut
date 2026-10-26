# OxCut Roadmap

A native desktop app (Windows first) that takes a batch of flatbed scans (JPEG, PNG, TIFF incl. 16-bit) holding one or more photo prints or postcards, detects each item, deskews it, and exports it as its own image. It runs either fully automatically or after the user adjusts rotatable marquees in a review UI.

The detection pipeline is a Rust port of **ScanSplitter v7** (https://github.com/Madnex/ScanSplitter, GPL-3.0). The UI is a Tauri 2 app with a Vue 3 frontend and a Fabric.js canvas editor. Full brief: `OXCUT_PLAN.md`.

Each phase ends with something you can run and check against the definition of done, so progress stays verifiable.

## Note to the Claude Code session (read this first)

- **UI shell first, then the engine behind it.** (Changed 2026-10-26 at the user's request; the original plan was engine and CLI first.) Phase 0.5 builds the Tauri app with mock data so there is an exe to double-click. The engine is still a pure library crate with a CLI, and Phases 1 and 2 continue as written. Wire the real engine into the UI in Phase 3.
- **Port behaviour, not Python idioms.** Before writing detection code, read `docs/specs/detector-v7.md`, `detector_v7.py`, `detector_v5.py`, `detector_v3.py` and the MobileSAM loading/inference code in `reference/ScanSplitter`. Record every deviation in `docs/detector-v7-port.md`.
- **Debug output for every detector stage** (PNG with overlays + JSON of boxes), reachable from the CLI.
- **No OpenCV.** Use `image` / `imageproc` / `ndarray`. If a routine has no Rust equivalent, implement it.
- **Ask before adding any dependency that pulls in a C/C++ build** (other than `ort`).
- **No ML weights in the repo or installer.** Models are downloaded on first use and checksum-verified.
- **Never modify the user's original files.** Always write to an output directory.
- **Commit after each phase** with a short summary in `docs/PROGRESS.md`.
- **License is GPL-3.0** (derivative of ScanSplitter). Keep attribution in README.

Formatting preference: no em dashes in output.

## Project status

Resume from the checkboxes: the first unchecked `[ ]` item, read top to bottom, is the next thing to do. Checked `[x]` items are done and confirmed working. Don't redo checked work; don't skip ahead of the first unchecked box.

## Phase 0: Environment setup

- [x] Install **Visual Studio 2022 Build Tools** (VCTools workload). _(Already installed and verified for the VST project: cl.exe 14.44 + Win SDK. `cl` is not on the plain PowerShell PATH, which is expected.)_
- [x] Install **Git**. _(git 2.54.0.)_
- [x] Install **Rust** via rustup, stable MSVC target: `winget install Rustlang.Rustup`, then `rustup default stable-x86_64-pc-windows-msvc` and `rustup component add clippy rustfmt`. Verify `rustc --version`, `cargo --version`. _(rustc/cargo 1.99.1, clippy + rustfmt installed.)_
- [x] Install **Node.js LTS**: `winget install OpenJS.NodeJS.LTS`. Verify `node --version`, `npm --version`. _(node 24.22.1, npm 11.19.1.)_
- [x] Confirm the **WebView2 runtime** is present (Settings -> Apps -> "Microsoft Edge WebView2 Runtime"). _(155.0.4283.62.)_
- [x] Install the **Tauri CLI**: `cargo install tauri-cli --version "^2"`. Verify `cargo tauri --version`. _(tauri-cli 2.12.4.)_
- [x] Install **uv**: `winget install astral-sh.uv` (only for running the reference ScanSplitter side by side). _(uv 0.13.5.)_
- [x] Clone the **reference checkout** (gitignored): `git clone https://github.com/Madnex/ScanSplitter reference/ScanSplitter`, then `uv sync` inside it, and confirm `uv run scansplitter process ...` works. _(scansplitter 0.8.0, CPython 3.13.17 venv; `uv run scansplitter --help` works.)_
- [x] (Optional) `cargo install cargo-nextest cargo-watch`. _(cargo-nextest 0.9.149, cargo-watch 8.5.3.)_

## Phase 0.5: UI shell with mock data

Goal: double-click `oxcut-app.exe` and see the whole interface, working on sample data. Nothing here touches real scans yet.

- [x] Scaffold `app/` with Tauri 2 + Vue 3 + TypeScript; crate `oxcut-app`, exe `oxcut-app.exe`, product name "OxCut", added to the Cargo workspace.
- [x] Layout: scan queue (left), Fabric.js review canvas (centre), settings and model manager (right), status bar, first-run welcome dialog.
- [x] Review canvas on mock scans: rotatable/resizable marquees, add, delete, split, straighten, zoom/pan, Shift-snap rotation, Enter/arrow/Delete keys.
- [x] Light and dark themes follow the system.
- [x] Release exe builds (`cd app; cargo tauri build --no-bundle` -> `target\release\oxcut-app.exe`, 9 MB), launches, and renders the UI; verified by screenshot including box selection.
- [x] Commit.

## Phase 1: Workspace + v3 port + CLI

Goal: `oxcut detect scan.jpg --mode v3 --debug out/` writes `boxes.json` (rotated rects: cx, cy, w, h, angle, in source px) and `debug.png` with boxes drawn.

- [x] `git init`; create the workspace `Cargo.toml` with `crates/oxcut-core`, `crates/oxcut-models`, `crates/oxcut-cli` (binary `oxcut`). _(Edition 2024, GPL-3.0-or-later matching ScanSplitter; builds and is clippy clean.)_
- [x] Add `LICENSE` (GPL-3.0), `README.md` with ScanSplitter attribution, `.gitignore` (`target/`, `reference/`, `app/node_modules/`).
- [x] Create `docs/PROGRESS.md` and `docs/detector-v7-port.md`.
- [x] Read the reference detector spec and v3 source before writing code. _(Notes, plan corrections and parity risks in `docs/detector-v7-port.md`. v3/v5 live in `detector.py`, not separate files.)_
- [x] Image I/O: load JPEG, PNG, TIFF (8 and 16-bit) preserving bit depth. _(`io.rs`; 16-bit TIFF round-trip test; detection copy truncates 16->8 like PIL.)_
- [x] Downsample for detection (1800 px long side, `INTER_AREA`) and map boxes back to full resolution. _(`resize.rs`, bit-identical to cv2 on 4 test images; `RotatedRect::scaled` maps back.)_
- [ ] Port v3 **background model**: platen/paper colour in Lab sampled from the scan border.
- [ ] Port v3 **dense non-background region** detection.
- [ ] Port v3 **gutter split** for touching prints.
- [ ] Port v3 **edge snap** of box sides to long physical edges.
- [ ] `oxcut detect` with `--mode v3` and `--debug` (per-stage overlays + JSON).
- [ ] Crop: rotate full-res around box centre, axis-aligned crop, optional inset px, preserve bit depth. `oxcut crop scan.tif --boxes boxes.json -o out/ --format same|jpg|png|tiff`.
- [ ] **Done when:** v3 boxes within Ã‚Â±2 px / Ã‚Â±0.5Ã‚Â° of Python v3 on the synthetic fixtures; `cargo clippy` clean; unit tests for Lab background model, gutter split, edge snap.
- [ ] Commit + `docs/PROGRESS.md` entry.

## Phase 2: Model registry, MobileSAM via `ort`, v5, v7, benchmark

- [ ] `oxcut-models` **registry** (`ModelSpec { id, filename, url, sha256, size_bytes }`). Copy the exact Acly/MobileSAM URLs and SHA-256 values from ScanSplitter's download code; don't guess.
- [ ] Storage in `%APPDATA%\oxcut\models\` via `directories::ProjectDirs::from("app", "oxcut", "oxcut")`.
- [ ] `ensure_model(id, progress)`: verify existing file, else stream to `.part`, verify SHA-256, atomic rename. Never load a file that fails the checksum.
- [ ] CLI: `oxcut models status|download|verify|path`.
- [ ] Offline / failed download degrades to v3 with a clear message, never crashes.
- [ ] **MobileSAM inference** via `ort` 2.x: same preprocessing (resize/pad to 1024, normalisation) and point/box prompting as the Python; cache the image embedding per scan.
- [ ] Port **v5**: v3/MobileSAM anchors, colour-independent texture pass, closed-frame pass for pale photos, texture-island splitting, tightly prompted segmentation near scan edges, fallback to the conservative anchor on conflict.
- [ ] Port **v7** (default mode): v5 anchors; v3/chroma alternative proposals scored by boundary brightness step; relative rejection of prints seen through glassine sleeves; per-side tightening onto the inner photographic edge.
- [ ] Expose `DetectionMode { V3, V5, V7 }` with `requires_models()`.
- [ ] `oxcut bench`: port `benchmarks/evaluate.py` semantics (rotated-box Hungarian matching, box quality, F1@0.50, F1@0.85, count accuracy) on `benchmarks/robustness/` fixtures plus real scans kept outside the repo. A/B against `uv run scansplitter process`.
- [ ] Check the white-lid acceptance cases (see below) on real scans.
- [ ] **Done when:** `oxcut bench` v7 box quality within ~3 points of Python v7 on the same fixtures; on a clean machine `oxcut models download` fetches, verifies and loads both MobileSAM files; a corrupted file is detected and re-fetched.
- [ ] Commit + `docs/PROGRESS.md` entry.

## Phase 3: Tauri 2 shell (batch / auto mode)

- [ ] Scaffold with `npm create tauri-app@latest` (Vue + TypeScript) inside `app/`.
- [ ] Commands: pick files/folder, run detection on a batch in a `rayon` pool, stream per-file progress via `tauri::ipc::Channel`, write outputs.
- [ ] **Model manager** panel: status, size, download progress, errors. Modes needing a missing model show "download required"; fall back to v3.
- [ ] First-run flow: welcome screen -> offer MobileSAM download (show size) -> v3 until done.
- [ ] Settings: detection mode, min/max item area (% of scan), inset px, output format (same / JPEG quality / PNG / TIFF), naming pattern `{source}-{n}`, DPI/EXIF passthrough.
- [ ] **Done when:** 50 scans process without blocking the UI; outputs open in Windows Photos and IrfanView; 16-bit TIFF stays 16-bit on "same format"; first run with no network works in v3.
- [ ] Commit + `docs/PROGRESS.md` entry.

## Phase 4: Review mode (marquee editor)

- [ ] Per-scan Vue view: scan on a Fabric.js canvas, one rotatable/resizable rect per box.
- [ ] Tools: add box, delete box, split box in half, re-detect, snap angle to 0Ã‚Â° with a modifier key, zoom/pan.
- [ ] Keyboard: Enter = approve & advance, arrows = move through queue, Delete = remove box.
- [ ] Flags (warnings): no item found, box touches scan edge, unusual aspect ratio/size, overlapping boxes. Auto-approve unflagged scans.
- [ ] **Done when:** a scan can be fully corrected by mouse + keyboard without touching settings.
- [ ] Commit + `docs/PROGRESS.md` entry.

## Phase 5: Projects, pairing, export

- [ ] Project persistence in the app data dir (JSON, or SQLite via `rusqlite`): scans, boxes, approval state, per-crop filename/caption.
- [ ] **Front/back pairing**: link a crop on one scan to a crop on another; export as `{name}-front` / `{name}-back`.
- [ ] "Auto-pair" helper: match by order across consecutive scans, user confirms.
- [ ] Export approved crops only, format per settings, optional JSON/CSV manifest.
- [ ] **Done when:** a 20-postcard project round-trips (close app, reopen, export pairs).
- [ ] Commit + `docs/PROGRESS.md` entry.

## Phase 6: Later

- [ ] Orientation model (0/90/180/270) via `ort`, registered in `oxcut-models`.
- [ ] Settings toggle to pre-download all models; custom mirror URL.
- [ ] macOS/Linux builds, code signing, auto-update.
- [ ] Album Splitter mode.

## Acceptance criteria (from the user)

- Scanner lid is **white**. White-bordered prints on a white platen: v7's per-side tightening must land on the photo, not the paper, and the frame pass must still detect the item.
- One or several items per bed; items may be skewed; items may touch the bed edge (clip, don't drop).
- Input: JPEG, PNG, TIFF (8/16-bit, Epson flatbed). Output: same format or user-chosen.
- Postcards: both sides scanned on separate beds; pairing is a project feature.
- Files only, no TWAIN/WIA scanning from the app.
- Orientation (which way is up) is out of scope for v1.

## Resources

- **ScanSplitter** (reference implementation): https://github.com/Madnex/ScanSplitter, especially `docs/specs/detector-v7.md` and `benchmarks/evaluate.py`.
- **Acly/MobileSAM** ONNX exports on Hugging Face.
- **Crates:** `image`, `imageproc`, `ndarray`, `palette`, `rayon`, `fast_image_resize`, `kamadak-exif` / `little_exif`, `serde`, `clap`, `anyhow` / `thiserror`, `tracing`, `ort` 2.x (evaluate `load-dynamic` for distribution), `reqwest` (rustls) + `sha2`, `directories`.
- **Tauri 2 docs** (commands, `ipc::Channel`, `app_data_dir()`), **Fabric.js** docs for the editor.
