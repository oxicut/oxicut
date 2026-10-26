# Oxcut — project brief for Claude Code (Windows)

A native, cross-platform (Windows first) desktop app that takes a batch of flatbed scans
(JPEG, PNG, TIFF incl. 16-bit) containing one or more photo prints / postcards, detects each
item, deskews it, and exports it as a separate image — either fully automatically or after the
user adjusts rotatable marquees in a review UI.

The detection pipeline is a Rust port of **ScanSplitter v7** (https://github.com/Madnex/ScanSplitter,
GPL-3.0). The UI is a Tauri 2 app with a Vue 3 frontend and a Fabric.js canvas editor, mirroring
ScanSplitter's React + Fabric.js editor.

**License:** this is a derivative of ScanSplitter, so the project is **GPL-3.0**. Add a LICENSE
file on day one and keep attribution in README.

**Name:** Oxcut. Crates `oxcut-core`, `oxcut-models`, `oxcut-cli`; CLI binary `oxcut`;
Tauri identifier `app.oxcut`; app data directory `%APPDATA%\oxcut\` (Tauri `app_data_dir()`).

---

## 0. Ground rules for Claude Code

- Read `docs/specs/detector-v7.md`, `src/scansplitter/detector_v7.py`, `detector_v5.py`,
  `detector_v3.py`, and the MobileSAM loading/inference code in the reference checkout **before**
  writing any detection code. Port behaviour, not Python idioms.
- Build the engine as a **pure library crate with a CLI** first. The GUI comes after the CLI
  produces correct boxes on real scans. Do not start the Tauri shell until Phase 2 passes.
- Every detector stage gets a debug output (PNG with overlays, JSON of boxes) reachable from the CLI.
- Prefer `image`/`imageproc`/`ndarray` over OpenCV bindings. No `opencv` crate: it makes Windows
  builds and distribution painful. If a specific OpenCV routine has no Rust equivalent, implement it.
- **No ML weights in the repo or the installer.** Models are fetched on first use (see §3).
- Keep the user's original files untouched; always write to an output directory.
- Ask the user before adding any dependency that pulls in a C/C++ build (other than `ort`).
- Commit after each phase with a short summary in `docs/PROGRESS.md`.

---

## 1. Windows environment setup

Run these in **PowerShell**. Verify each step with the command shown.

1. **Visual Studio Build Tools** (MSVC toolchain, required by Rust and Tauri)
   - `winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`
   - Verify: "x64 Native Tools Command Prompt" exists in the Start menu, or `cl.exe` is found from a Developer PowerShell.
2. **Rust** via rustup (stable, MSVC target)
   - `winget install Rustlang.Rustup` then `rustup default stable-x86_64-pc-windows-msvc`
   - Verify: `rustc --version`, `cargo --version`
   - Add: `rustup component add clippy rustfmt`
3. **Node.js LTS** (frontend tooling): `winget install OpenJS.NodeJS.LTS` — verify `node --version`, `npm --version`
4. **WebView2 runtime**: pre-installed on Windows 10 (1803+) / 11. Verify in Settings → Apps → "Microsoft Edge WebView2 Runtime". If missing, install the Evergreen bootstrapper from Microsoft.
5. **Tauri CLI**: `cargo install tauri-cli --version "^2"` (or `npm i -D @tauri-apps/cli`). Verify: `cargo tauri --version`
6. **Git**: `winget install Git.Git`
7. **uv** (only for running the reference ScanSplitter and its benchmark scripts side by side):
   - `winget install astral-sh.uv`
8. **Reference checkout** (not part of our repo; gitignored):
   - `git clone https://github.com/Madnex/ScanSplitter reference/ScanSplitter`
   - `cd reference/ScanSplitter && uv sync` so `uv run scansplitter process ...` works for A/B comparisons.

Optional but recommended: `cargo install cargo-nextest` (faster tests), `cargo install cargo-watch`.

---

## 2. Repository layout

```
oxcut/
├── Cargo.toml                 # workspace
├── LICENSE                    # GPL-3.0
├── README.md
├── docs/
│   ├── PROGRESS.md            # phase log
│   ├── detector-v7-port.md    # our notes on the port, deviations from the Python
│   └── ...
├── crates/
│   ├── oxcut-core/            # detection, deskew, crop, export — no I/O policy, no UI
│   ├── oxcut-models/          # model registry, download/verify, MobileSAM (+ later orientation) via `ort`
│   └── oxcut-cli/             # `oxcut detect|crop|bench|models ...`
├── app/                       # Tauri 2 app
│   ├── src-tauri/             # Rust side: commands, project store, job runner
│   └── src/                   # Vue 3 + TypeScript + Fabric.js
├── benchmarks/                # ported fixtures + evaluate tool (see Phase 2)
└── reference/                 # gitignored: ScanSplitter checkout
```

Suggested crates:
- `image` (decode/encode JPEG, PNG, TIFF incl. 16-bit), `imageproc` (threshold, morphology,
  contours, region_labelling, geometry::min_area_rect, gradients), `ndarray`, `palette` (Lab),
  `rayon`, `fast_image_resize` (downsampling), `kamadak-exif` or `little_exif` (EXIF/DPI
  passthrough), `serde`/`serde_json`, `clap` (CLI), `anyhow`/`thiserror`, `tracing`.
- `ort` 2.x for ONNX Runtime (default feature downloads prebuilt ONNX Runtime binaries at
  build time; evaluate `load-dynamic` for distribution so the runtime DLL ships next to the exe).
- `reqwest` (rustls, streaming) + `sha2` for model downloads; `directories` for the data dir.

---

## 3. Models: download on first use (not bundled)

`oxcut-models` owns a small **model registry** and nothing else knows URLs:

```rust
pub struct ModelSpec { id: &'static str, filename: &'static str, url: &'static str,
                       sha256: &'static str, size_bytes: u64 }
```

- Entries: MobileSAM image encoder + prompt decoder (the checksum-pinned ONNX exports from
  **Acly/MobileSAM** on Hugging Face that ScanSplitter uses; ~43 MB total), and later the
  orientation model (~80 MB). **Copy the exact URLs and SHA-256 values from ScanSplitter's model
  download code** rather than guessing.
- Storage: `<app_data_dir>/models/<filename>` (Windows: `%APPDATA%\oxcut\models\`). The CLI uses
  the same directory via `directories::ProjectDirs::from("app", "oxcut", "oxcut")`.
- `ensure_model(id, progress: impl FnMut(Progress))`: if the file exists and its SHA-256
  matches, return the path; otherwise download to `<filename>.part` with streaming progress,
  verify SHA-256, atomically rename. Never load a model whose checksum fails; delete and re-download.
- `oxcut models status|download|verify|path` for the CLI.
- In the app, a **Model manager** panel shows status, size, download progress and errors, and
  triggers download on first run or on first use of a mode that needs it. Detection modes that need
  a missing model are shown as "download required" and the UI falls back to **v3** until they exist.
- Offline/failed download must degrade gracefully to v3 with a clear message, never crash.
- Optional later: a settings toggle to pre-download all models at install time, and a custom
  mirror URL (ScanSplitter keeps backup mirrors in its GitHub releases).

---

## 4. Phase plan

### Phase 1 — Workspace + v3 port + CLI
Goal: `oxcut detect scan.jpg --mode v3 --debug out/` writes `boxes.json` (rotated rects:
cx, cy, w, h, angle, source px) and `debug.png` with boxes drawn.

- Port **v3** (model-free, background-aware): model platen/paper colour in Lab from the scan
  border, find dense non-background regions, split touching prints at narrow gutters, snap box
  sides to long physical edges. Follow `detector_v3.py` exactly; note every deviation in
  `docs/detector-v7-port.md`.
- Downsample for detection (~1500 px long side, confirm against the Python), map boxes back to
  full resolution.
- Crop: rotate full-res around the box centre, axis-aligned crop, optional inset px. Preserve
  bit depth. `oxcut crop scan.tif --boxes boxes.json -o out/ --format same|jpg|png|tiff`.

### Phase 2 — Model registry, MobileSAM via `ort`, then v5, then v7
- Implement `oxcut-models` as in §3 (registry, download, verify, CLI subcommands).
- MobileSAM inference: same preprocessing (resize/pad to 1024, normalisation) and point/box
  prompting the Python uses; cache the image embedding per scan so repeated prompts are cheap.
- Port **v5**: v3/MobileSAM anchors, colour-independent texture pass, closed-frame pass for pale
  photos, texture-island splitting, tightly prompted segmentation near scan edges, fallback to
  the conservative anchor on conflict.
- Port **v7**: v5 anchors; v3/chroma alternative proposals scored by the brightness step along
  their boundary; relative rejection of prints seen through glassine sleeves; **per-side
  tightening onto the inner photographic edge** (excludes white/scalloped borders). This is the
  default mode.
- **Benchmark**: port `benchmarks/evaluate.py` semantics (rotated-box matching via Hungarian
  assignment, box quality, F1@0.50, F1@0.85, count accuracy) as `oxcut bench`. Use the
  repo's synthetic `benchmarks/robustness/` fixtures plus the user's real scans (kept outside
  the repo). Target: within a few points of the Python v7 on the same inputs. Run the Python
  side via `uv run scansplitter process` for A/B.
- Expose `DetectionMode { V3, V5, V7 }`; `V5`/`V7` report `requires_models()`.

### Phase 3 — Tauri 2 shell (batch / auto mode)
- `npm create tauri-app@latest` with the Vue + TypeScript template, inside `app/`.
- Commands: pick files/folder, run detection on a batch in a background thread pool (`rayon`),
  stream per-file progress to the frontend with `tauri::ipc::Channel`, write outputs.
- First-run flow: welcome screen → model manager offers to download MobileSAM (shows size) →
  until done, detection runs in v3.
- Settings: detection mode, min/max item area (% of scan), inset px, output format
  (same-as-source / JPEG quality / PNG / TIFF), naming pattern `{source}-{n}`, DPI/EXIF passthrough.

### Phase 4 — Review mode (marquee editor)
- Vue view per scan: the scan image on a Fabric.js canvas, one rotatable/resizable rect per box,
  plus: add box, delete box, split box in half, re-detect, snap angle to 0° with a modifier key,
  zoom/pan.
- Keyboard: Enter = approve & advance, arrows = move through queue, Delete = remove box.
- Flags (warnings, not errors), mirroring ScanSplitter: no item found, box touches scan edge,
  unusual aspect ratio/size, overlapping boxes. Auto-approve unflagged scans.

### Phase 5 — Projects, pairing, export
- Project = named collection persisted as JSON (or SQLite via `rusqlite`) in the app data dir:
  scans, boxes, approval state, per-crop filename/caption.
- **Front/back pairing**: link a crop on one scan to a crop on another scan (postcards: fronts
  on one bed, backs on another). Export naming `{name}-front` / `{name}-back`. Offer an
  "auto-pair" helper: match by order within consecutive scans, user confirms.
- Export: approved only; format per settings; optional JSON/CSV manifest.

### Phase 6 — Later
- Orientation model (0/90/180/270) via `ort`, registered in `oxcut-models` like MobileSAM.
- macOS/Linux builds, code signing, auto-update.
- Album Splitter mode.

---

## 5. Detection requirements from the user (acceptance criteria)

- Scanner lid is **white**. White-bordered prints are on a white platen: the paper edge is still
  visible, the interior border is pale. v7's per-side tightening must land on the photo, not the
  paper; the "frame" pass must still detect the item.
- One or several items per bed; items may be skewed; items may touch the bed edge (clip, don't drop).
- Input: JPEG, PNG, TIFF (8/16-bit, from an Epson flatbed). Output: same format or user-chosen.
- Postcards: both sides scanned on separate beds; pairing is a project feature.
- Files only — no TWAIN/WIA scanning from the app.
- Orientation (which way is up) is out of scope for v1.

---

## 6. Definition of done per phase

- Phase 1: v3 boxes within ±2 px / ±0.5° of Python v3 on the synthetic fixtures; `cargo clippy`
  clean; unit tests for Lab background model, gutter split, edge snap.
- Phase 2: `oxcut bench` reports v7 box quality within ~3 points of the Python v7 on the
  same fixtures; on a clean machine `oxcut models download` fetches, verifies and loads both
  MobileSAM files, and a corrupted file is detected and re-fetched.
- Phase 3: batch of 50 scans processes without blocking the UI; outputs open correctly in
  Windows Photos and IrfanView; 16-bit TIFF stays 16-bit when "same format"; first run without
  network still works in v3.
- Phase 4: a scan can be fully corrected by mouse+keyboard without touching settings.
- Phase 5: a postcard project of 20 cards round-trips (close app, reopen, export pairs).

---

## 7. First commands for Claude Code

```
cargo new --lib crates/oxcut-core
cargo new --lib crates/oxcut-models
cargo new crates/oxcut-cli
# write workspace Cargo.toml, LICENSE (GPL-3.0), .gitignore (target/, reference/, app/node_modules/)
# read reference/ScanSplitter/docs/specs/detector-v7.md and the detector_v3/v5/v7 sources
# start Phase 1
```
