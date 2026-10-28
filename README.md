# OxiCut

A native Windows app that splits flatbed scans of photo prints and postcards into separate, straightened images. Put several prints on the scanner glass, scan once, and OxiCut finds each print, squares it up, and saves it as its own file. It runs fully automatically, or you can check and adjust every box first.

Status: early development (version 0.0.1). Windows only so far, and there is no installer yet: you build it from source (see [Building](#building-from-source)). Progress is tracked in [`roadmap.md`](roadmap.md) and [`docs/PROGRESS.md`](docs/PROGRESS.md).

## Based on ScanSplitter

OxiCut is a branch of [ScanSplitter](https://github.com/Madnex/ScanSplitter) by Madnex (GPL-3.0), version 0.8.0 (the checkout it was ported from is commit `b435ebb`). ScanSplitter is a Python and OpenCV tool with a web interface; its detection is what OxiCut builds on:

- **The detectors.** ScanSplitter's v3 (colour and edge based), v5 (MobileSAM anchors with texture and frame passes) and v7 (several proposal sets scored against each other, with per-side tightening onto the photograph's own edge so white and scalloped borders are left out) are reimplemented in Rust, stage by stage, following the Python code and its `docs/specs/detector-v7.md`.
- **The benchmark.** OxiCut's `bench` command reproduces ScanSplitter's scoring (rotated-box matching, box quality, F1, count accuracy) and runs on its fixtures, so the two can be compared directly.
- **The models.** The same MobileSAM ONNX files, from the same pinned Hugging Face revision and with the same checksums.

How closely the port follows the original, and every place it deviates, is written down in [`docs/detector-v7-port.md`](docs/detector-v7-port.md). Because OxiCut is a derivative work it is licensed under the GPL-3.0 as well.

## What is different

**A desktop app, not a local web server.**
- One native executable (about 36 MB, the ONNX Runtime is built in). No Python, no `uv`, no browser tab, nothing listening on a port.
- Detection runs on a worker pool and streams progress, so the window stays responsive: 50 scans on the full v7 detector load without the window ever freezing.

**No OpenCV.** Everything OpenCV did for ScanSplitter (Lab conversion, Otsu, contours, Canny, Hough lines, warps, resizing) is written in Rust, including the details that change results, such as OpenCV's contour tracing, its interpolation rounding and its Gaussian kernel sizes. The stages were checked against ScanSplitter's own output on its 114 test fixtures: each stage is identical on 110 to 114 of them, and the rest are explained in the port notes.

**Close to the original's accuracy.** On ScanSplitter's holdout benchmark (box quality at a 0 px trim) the Rust v7 scores 88.2%, against the 88.8% ScanSplitter publishes for its Python v7.

**A detector that needs no download.** *v7-lite* is new in OxiCut: v3's boxes plus v7's inner-edge tightening, with no model. It is fast, works offline, and is the default; the full v7 is one click away once the models are on disk.

**Models stay out of the app.** They are downloaded once, from a pinned revision, checked against a SHA-256 and kept in `%APPDATA%\oxicut\models`, shared by the app and the command line. A damaged or truncated file is detected and fetched again. The welcome dialog offers the download (about 45 MB) on first run; until then everything else works.

**Crops come from the original pixels.** Each crop is sampled straight from the full-resolution scan in one step, in the scan's own bit depth, so a 16-bit TIFF stays 16-bit. Output can be the scan's own format, JPEG, PNG or TIFF.

**The scan's metadata comes along.** Resolution (DPI), colour profile and EXIF are written into the crops in every format, so a 600 dpi scan gives 600 dpi crops. The scan's embedded thumbnail and pixel-size fields are dropped, since they describe the whole scan. Each crop can also say `OxiCut 0.0.1` in its Software field (a separate option, on by default).

**A review workflow built for batches.**
- Add files, a folder, or drop them on the window; scans are detected as they arrive.
- Scans with nothing suspicious are approved automatically; the ones flagged (nothing found, touches the scan edge, unusual shape, overlapping boxes) wait for you. `Enter` approves and moves to the next.
- Rotatable, resizable boxes on a Fabric.js canvas, with add, split, delete and straighten tools, undo and redo per scan, and Shift to snap rotation to 90 degrees.
- **Loupes.** A round 4x magnifier follows any resize handle you drag, and while you move or rotate a box, all four of its corners are magnified at once (one loupe in each corner of the view) so you can line up whichever corner you care about.
- The scan list and the settings panel fold into slim rails that still show what matters: how many scans are approved and how many need review, a thumbnail per scan with its status, the detector and the export format.
- A Settings dialog (gear icon, or `Ctrl+,`) lets you follow Windows, or always use a light or dark look.
- Closing the window asks first if it would lose work: scans never exported, edited after export, edited without approval, or an export in progress.
- Your originals are never modified, and existing files are never overwritten (a new file gets ` (2)` in its name).

## What ScanSplitter has that OxiCut does not (yet)

- PDF input.
- Automatic 90/180/270 degree orientation (planned; the orientation model is already in the registry).
- Persistent projects, archival metadata (dates, places, captions), lossless masters and manifests. OxiCut keeps nothing between sessions yet; projects and front/back pairing for postcards are Phase 5 of the roadmap.
- Album Splitter mode, the separate v4 and v5 modes, and the experimental OpenRouter vision-model mode.
- macOS and Linux (nothing in the stack ties OxiCut to Windows, but it is only built and tested there).

Small differences to expect on the same scan: OxiCut decodes JPEG with a different library than Pillow, so on hard images boxes can differ by a few pixels, and its default trim is 0 px (ScanSplitter's 10 px shrinks every box).

## Using the app

1. **Add** scans (JPEG, PNG or TIFF, 8 or 16 bit) with *Add scans*, *Folder*, or by dropping them on the window.
2. **Check** the boxes. Drag, resize or rotate any of them; `Enter` approves and moves on.
3. **Export** the approved scans. Crops go to an `OxiCut output` folder beside each scan, or to a folder you choose, named `{source}-{n}` unless you change the pattern.

| Keys | |
|---|---|
| `Enter` | approve and go to the next scan |
| `←` `→` / `↑` `↓` | previous / next scan |
| `Delete` | delete the selected box |
| `Ctrl+Z`, `Ctrl+Y` (or `Ctrl+Shift+Z`) | undo, redo for the open scan |
| `Shift` while rotating | snap to 90 degrees |
| scroll, drag the background | zoom, pan |
| `Ctrl+B`, `Ctrl+Shift+B` | fold the scan list, the settings panel |

[`docs/TESTING.md`](docs/TESTING.md) is a longer walk through everything worth trying, and what is known to be missing.

## Command line

```
oxicut detect scan.tif --mode v7-lite --debug out\detect   # boxes.json, debug.png, stages.png, masks
oxicut detect scan.tif --mode v7 -o boxes.json             # just the boxes
oxicut crop scan.tif --boxes boxes.json -o out\crops       # same format as the scan, 16-bit kept

oxicut models status | download | verify | path            # the MobileSAM files
oxicut bench --mode v7 --suite robust-holdout              # ScanSplitter's benchmark
```

`--mode` is `v3` (the command-line default), `v7-lite` or `v7` (needs the models). `detect` also takes `--min-area` and `--max-area` (percent of the scan), `--inset` and `--padding`. `crop` takes `--format same|jpg|png|tiff`, `--quality`, `--no-metadata` and `--no-software-tag`. Set `OXICUT_MODELS_DIR` to keep the models somewhere else.

## Building from source

You need Rust (stable, MSVC), Node.js, and the WebView2 runtime that ships with current Windows.

```
cargo build --release -p oxicut-cli       # target\release\oxicut.exe
cd app
npm install
cargo tauri build --no-bundle             # target\release\oxicut-app.exe
```

`cargo test --workspace` runs the Rust tests, including the stage-by-stage comparisons against ScanSplitter's output (`benchmarks/golden/`); `npm test` in `app` runs the interface logic tests.

## Credits and license

The detection pipeline, its benchmark methodology and the choice of MobileSAM are ScanSplitter's, by Madnex. MobileSAM weights are from [Acly/MobileSAM](https://huggingface.co/Acly/MobileSAM).

OxiCut is licensed under the GNU General Public License v3.0, as a derivative work of ScanSplitter. See [LICENSE](LICENSE).
