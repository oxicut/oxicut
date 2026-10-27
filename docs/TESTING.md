# Trying OxiCut by hand

This is the first version you can use on real scans. It finds prints with the **v7-lite detector** by default (ScanSplitter's v3 plus v7's edge tightening), or with the full **v7 detector** once you download the MobileSAM models (Models tab, about 45 MB); plain v3 can also be chosen in the Detection tab. You can correct the boxes and export the straightened crops. Your original scans are never changed.

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
3. **Fix a box.** Drag it, undo and redo with Ctrl+Z and Ctrl+Y (or the toolbar buttons; each scan has its own history, and Re-detect can be undone), drag a corner or side handle to resize (a round magnifier shows the area around the handle, so you can place it exactly), use the round handle to rotate (hold Shift to snap to 90 degrees). The box tools are in the rail to the left of the scan: *Box* adds one, *Split* halves the selected box, *Delete* (or the Delete key) removes it, *Straighten* snaps it square, *Re-detect* starts that scan over.
4. **Approve.** `Enter` approves the scan and moves to the next. Arrow keys move through the list without approving. *Approve N unflagged* approves all the quiet ones at once.
5. **Export.** *Export approved* (top right, or the Export tab). Crops go to an `OxiCut output` folder beside each scan, or to the folder you pick. Names follow the pattern (default `{source}-{n}`). An existing file is never overwritten: the new one gets ` (2)` in its name. *Show in folder* opens the result.
6. **Settings** (right panel) are saved automatically. Detection settings apply to scans detected from then on; *Re-detect all* applies them to the list.
7. **Closing.** Closing the window asks first when work would be lost: approved scans that were never exported, scans edited after they were exported, scans with edits that are not approved, or an export still running. It offers *Keep working* and *Close without exporting*. Scans that were only added and never touched, and lists that are fully exported, close without a question. A crash, a force-kill or a Windows shutdown cannot be intercepted, and nothing is saved between sessions yet.

Things worth checking in particular:

- **Try v7.** Download the models in the Models tab, choose "v7 · most accurate" in the Detection tab, and use *Re-detect all*. It should find prints v7-lite missed (on the test scans: the top-middle print of the tape scan and the second photo of the 16-bit TIFF) and takes a few seconds per scan. Please note any scan where v7 is worse than v7-lite.
- A white-bordered print on your white lid: does the box land on the photo or on the paper? v7-lite and v7 should sit on the photo where v3 ran onto the border.
- A 16-bit TIFF from the Epson: export as *Same as the scan* and open the result. It should still be 16-bit and open in Windows Photos and IrfanView.
- DPI and EXIF: scan something at 600 dpi, export it, and look at the crop in Windows (right click, Properties, Details). The resolution should read 600 dpi, and with *Write OxiCut into the Software field* ticked the "Program name" should read `OxiCut 0.0.1`. With *Keep DPI, colour profile and EXIF* unticked the crop should lose the dpi, the profile and the EXIF dates, and keep only the Program name. The version is also shown at the bottom right of the window.
- First run: the welcome dialog offers the v7 download. To see it again, close OxiCut and delete `%LOCALAPPDATA%pp.oxicutBWebViewDefaultocal storage`.
- Prints touching each other, or touching the edge of the bed.
- A big batch (dozens of scans): does the window stay responsive?

## Known gaps

- v7-lite is only part of ScanSplitter's v7 (no models): it finds the same number of prints as v3. The full v7 is much better at counting them, but a whole album page can still come out as one box, as in ScanSplitter.
- Both detectors can miss pale prints on a white background and can merge prints that touch. These are the same limits ScanSplitter's v3 has, which is where the detector comes from.
- EXIF is copied from JPEG and PNG scans only; from a TIFF scan the crops get the resolution and colour profile, not the EXIF (dates, scanner make).
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
