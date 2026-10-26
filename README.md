# OxCut

A native desktop app (Windows first) that splits flatbed scans containing one or more photo prints or postcards into separate, deskewed images. Works fully automatically or with a review UI of rotatable marquees.

Status: early development. See `roadmap.md` for progress and `OXCUT_PLAN.md` for the full brief.

## Command line

```
cargo build --release -p oxcut-cli          # target\release\oxcut.exe

oxcut detect scan.tif --debug out\detect    # boxes.json + debug.png, stages.png, masks
oxcut detect scan.tif -o boxes.json         # just the boxes
oxcut crop scan.tif --boxes boxes.json -o out\crops --format same
```

`detect` finds each print and writes its rotated box (centre, size, angle in source pixels). `crop` cuts every box out and straightens it, keeping 16-bit TIFFs 16-bit. Your scan is never modified. Options: `--min-area`/`--max-area` (percent of the scan), `--inset`, `--padding`, `--format same|jpg|png|tiff`, `--quality`.

Only the model-free `v3` detector exists so far; the MobileSAM-based v5 and v7 are planned (see `roadmap.md`).

## Desktop app

`cd app; cargo tauri build --no-bundle` builds `target\release\oxcut-app.exe`. It currently shows the interface on sample data and is not yet connected to the detector.

## Attribution

The detection pipeline is a Rust port of [ScanSplitter](https://github.com/Madnex/ScanSplitter) by Madnex (GPL-3.0), in particular its v3, v5 and v7 detectors and benchmark methodology.

## License

OxCut is a derivative work of ScanSplitter and is licensed under the GNU General Public License v3.0. See [LICENSE](LICENSE).
