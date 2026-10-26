# Trying OxiCut by hand

This is the first version you can use on real scans. It finds prints with the built-in **v7-lite detector** (ScanSplitter's v3 plus v7's edge tightening; plain v3 can be chosen in the Detection tab), lets you correct the boxes, and exports the straightened crops. Your original scans are never changed.

## Start it

Build once (about a minute after the first build):

```
cd app
cargo tauri build --no-bundle
```

Then double-click `target\release\oxicut-app.exe`. You can also start it on scans directly:

```
target\release\oxicut-app.exe D:\scans\batch1 D:\scans\one-more.tif
```

Close the app before rebuilding: Windows will not replace a running exe.

## What to try

1. **Add scans.** Use *Add scans...* or *Folder...*, or drag files and folders onto the window. JPEG, PNG and TIFF (8 and 16 bit) are read. Thumbnails appear as each scan is finished; detection runs a few scans at a time.
2. **Look at the result.** Click a scan. Orange boxes are the detected prints; an amber box has a warning (the reason is shown under the scan and in the bottom bar). Scans with nothing to flag are approved for you.
3. **Fix a box.** Drag it, drag a corner to resize, use the round handle to rotate (hold Shift to snap to 90 degrees). *+ Box* adds one, *Split* halves the selected box, *Delete* (or the Delete key) removes it, *Straighten* snaps it square, *Re-detect* starts that scan over.
4. **Approve.** `Enter` approves the scan and moves to the next. Arrow keys move through the list without approving. *Approve N unflagged* approves all the quiet ones at once.
5. **Export.** *Export approved* (top right, or the Export tab). Crops go to an `OxiCut output` folder beside each scan, or to the folder you pick. Names follow the pattern (default `{source}-{n}`). An existing file is never overwritten: the new one gets ` (2)` in its name. *Show in folder* opens the result.
6. **Settings** (right panel) are saved automatically. Detection settings apply to scans detected from then on; *Re-detect all* applies them to the list.

Things worth checking in particular:

- A white-bordered print on your white lid: does the box land on the photo or on the paper? v7-lite should sit on the photo where v3 ran onto the border; switch the detector in the Detection tab and use *Re-detect all* to compare the two on the same scans.
- A 16-bit TIFF from the Epson: export as *Same as the scan* and open the result. It should still be 16-bit and open in Windows Photos and IrfanView.
- Prints touching each other, or touching the edge of the bed.
- A big batch (dozens of scans): does the window stay responsive?

## Known gaps

- v7-lite is only part of ScanSplitter's v7. It does not yet weigh alternative proposals against each other, drop prints seen through glassine sleeves, or use the MobileSAM-based v5 anchors, so it can still miss or merge prints (v3 and v7-lite find the same number of prints). The Models tab can already download and verify the model files, but nothing uses them yet.
- Both detectors can miss pale prints on a white background and can merge prints that touch. These are the same limits ScanSplitter's v3 has, which is where the detector comes from.
- DPI and EXIF are not copied into the crops yet (the option is shown greyed out).
- The list of scans is not saved: closing the app forgets it (projects come later). Settings are kept.
- No front/back pairing yet (postcards).
- JPEG files are decoded by a different library than ScanSplitter used, so on hard test images the boxes can differ slightly from ScanSplitter's. TIFF and PNG decode identically.

## Where things are kept

| What | Where |
|---|---|
| Settings | `%APPDATA%\oxicut\settings.json` |
| Downloaded models | `%APPDATA%\oxicut\models` |
| Preview images (safe to delete) | `%LOCALAPPDATA%\oxicut\previews` |

Set `OXICUT_DATA_DIR`, `OXICUT_MODELS_DIR` or `OXICUT_CACHE_DIR` to move them.

## If something looks wrong

Note the file name, what you did, and what you expected. For a detection problem, the command line shows every stage:

```
target\release\oxicut.exe detect scan.tif --debug out\detect
```

and `out\detect\stages.png` shows each step's boxes over the scan.
