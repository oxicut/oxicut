# Trying OxiCut by hand

This is the first version you can use on real scans. It finds prints with **Standard (v7-lite)** by default (colour and edges plus tightening onto the photograph's own edge), or with **Careful (v7)** once you download its extra files (Downloads tab, about 45 MB); **Basic (v3)** can also be chosen in the Detection tab. You can correct the boxes and export the straightened crops. Your original scans are never changed.

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
3. **Fix a box.** Drag it, undo and redo with Ctrl+Z and Ctrl+Y (or the toolbar buttons; each scan has its own history, and Re-detect can be undone), drag a corner or side handle to resize (a round magnifier shows the area around the handle, so you can place it exactly; while you move a box or turn it, four magnifiers show all of its corners, one in each corner of the view), use the round handle to rotate (hold Shift to snap to 90 degrees). The box tools are in the rail to the left of the scan: *Box* adds one, *Split* halves the selected box, *Delete* (or the Delete key) removes it, *Straighten* snaps it square, *Closer* and *Wider* move every edge of the selected box a little in or out (to lose a white border, or to take in a little more), *Re-detect* starts that scan over.
4. **Approve.** `Enter` approves the scan and moves to the next. Arrow keys move through the list without approving. *Approve N unflagged* approves all the quiet ones at once.
5. **Export.** *Export approved* (top right, or the Export tab). Crops go to an `OxiCut output` folder beside each scan, or to the folder you pick. Names follow the pattern (default `{source}-{n}`). An existing file is never overwritten: the new one gets ` (2)` in its name. *Show in folder* opens the result.
6. **Settings** (right panel) are saved automatically. Detection settings apply to scans detected from then on; *Re-detect all* applies them to the list.
7. **Collapsing the side panels.** The « button on the scan list and the » button on the right panel (or Ctrl+B and Ctrl+Shift+B) fold them into slim rails that keep the key data in view: the scan list shows how many scans are approved (6/6), a warning count of scans still to review, and a thumbnail per scan with its status colour, the open one outlined; the right panel shows the detector, the export format, and a warning dot on Models when v7 is selected without its models (or the download percentage while it runs). Clicking a rail icon opens the panel on that tab. The choice is remembered, and zooming into a scan survives a panel opening or closing.
8. **Settings and theme.** The Settings button at the bottom right of the window (or Ctrl+,) opens the settings. Under Appearance pick *Follow system* (matches your Windows light or dark setting), *Light* or *Dark*; the whole window, including its title bar, changes at once, and the choice is remembered. Escape or *Close* closes the dialog.
9. **Projects.** The project name at the top of the window (My scans at first) is a button. Your scans, boxes and approvals are saved automatically, a second after each change, in a file named after the project in the OxiCut folder inside Documents (Show in folder reveals it). Close the app and open it again: everything is where you left it. To start again with an empty list, press Create new project at the bottom of the scan list (the folded list has a small button for it too); the project you leave stays saved. In the Projects window you can also switch, rename, remove one from the list (the file stays) or open a project file from anywhere. Copy a project file with its scans folder to another computer and it still finds them. Move a scan file away and reopen the project: that scan shows "file not found" and keeps its boxes. Starting OxiCut again with a project file (as double-clicking it does once installed) opens it in the window that is already open.
10. **About.** The "OxiCut 0.0.1" at the bottom right of the window is a button: it opens a small window with the name, the version, a button that opens the project page in your browser, and the credit line. Escape or Close shuts it. (The page only exists once the repository has been created and pushed; until then the browser shows GitHub's "not found" page.)
11. **Crops.** The Scan | Crops switch at the left end of the toolbar above the picture opens the Crops view, which shows the straightened pictures that an export will write, one card per box, with the file name each will get and its size in pixels. "This scan" shows the open scan, "All scans" every scan. Turn Left or Turn Right fixes a print that lies sideways or upside down (the box on the scan looks the same afterwards, the exported picture is turned; Ctrl+Z on the Scan tab undoes it), Remove deletes the box, and clicking a picture opens it larger (arrow keys step through, Escape closes). In the larger view, a line of buttons above Previous and Next has "Cut closer" and "Cut wider" (every edge of the crop moves a little in or out, for example to lose a white border; this changes the box on the scan too, one Undo step per click, and the picture refreshes), and "Show final result": it makes the actual full-size file, exactly as an export would write it with your current settings (format, quality, metadata), into a scratch folder, and shows it. You see the real pixels, can click the picture to see it at 100%, switch between Preview and Final result, and "Show file" reveals it in its folder; "Refresh final result" prepares it again after you change export settings. Nothing is exported by this; the scratch copies are replaced when you cut again and cleaned up after a day. A TIFF cannot be shown in the window, so it is shown through a full-size JPEG copy (the window says so). "Export this scan" writes only the open scan, approved or not. Exported crops get an Exported mark and a Show file button, and the Export tab lists what was written for each scan (file names and sizes; the names are kept in the project). The pictures are cut from the screen preview in a few milliseconds, with the same code as the export, so they show what will be written without writing anything.
12. **Closing.** Closing the window asks only while an export is running (images could be left half written). Everything else is already saved.

Things worth checking in particular:

- **Try Careful.** Download its files in the Downloads tab, choose "Careful (v7) · most accurate" under *Detection method* in the Detection tab, and use *Re-detect all*. It should find prints v7-lite missed (on the test scans: the top-middle print of the tape scan and the second photo of the 16-bit TIFF) and takes a few seconds per scan. Please note any scan where v7 is worse than v7-lite.
- A white-bordered print on your white lid: does the box land on the photo or on the paper? v7-lite and v7 should sit on the photo where v3 ran onto the border.
- A 16-bit TIFF from the Epson: export as *Same as the scan* and open the result. It should still be 16-bit and open in Windows Photos and IrfanView.
- DPI and EXIF: scan something at 600 dpi, export it, and look at the crop in Windows (right click, Properties, Details). The resolution should read 600 dpi, and with *Write OxiCut into the Software field* ticked the "Program name" should read `OxiCut 0.0.1`. With *Keep DPI, colour profile and EXIF* unticked the crop should lose the dpi, the profile and the EXIF dates, and keep only the Program name. The version is also shown at the bottom right of the window.
- First run: the welcome dialog offers the v7 download. To see it again, close OxiCut and delete `%LOCALAPPDATA%pp.oxicutBWebViewDefaultocal storage`.
- Prints touching each other, or touching the edge of the bed.
- A big batch (dozens of scans): does the window stay responsive?

## Making a release

Change `version` in the root `Cargo.toml` (it feeds the app, the installer and the command line), push, then on GitHub open Actions, choose *Release* and *Run workflow*. It runs the tests, builds the installer and publishes a release named `v<version>` with `OxiCut-setup.exe`. Running it again for the same version replaces that release. Install the result on a clean machine and check: it starts, the About window shows the version, and double-clicking a `.oxicut` file opens it.

## Date, place, caption and people

This is hidden behind the `photo-details` flag (see [`feature-flags.md`](feature-flags.md)). Start the app with it on, for example in PowerShell: `$env:OXICUT_FEATURES = "photo-details"`, then run the app (or put `"features": ["photo-details"]` in the settings file). **With the flag off, as in a normal run, check that none of it shows**: no Details tab on the right, no *About this picture…* on the crop cards, no line of date and place on them, and an export carries no XMP and keeps the scan's own date and description. With it on:

Open the **Details** tab (right panel) with a scan selected and fill in the fields: the line under *Date* says what OxiCut understood ("Saved as about 1980." for "circa 1980"; "No year found, so it is kept as you wrote it." when there is no year). In the **Crops** view each card shows the date and place in small type, and **About this picture…** opens that picture's own details (what you leave empty is taken from the scan, shown greyed out; **Use the scan's details** clears them). **Copy to the other scans** only fills what is still empty. Then export and check:

- in Windows (right click a saved JPEG, Properties, Details) *Date taken*, *Comments* or the description show the date and caption; a program that reads XMP (for example a photo library) shows the caption, the people as tags, and the place;
- a picture with its own caption carries that one, its neighbours carry the scan's;
- a PNG and a TIFF keep their details too (for a TIFF the caption is also in the description tag when it is plain ASCII);
- close the app and open it again: the details are still there, and the cards that have their own are marked;
- remove a crop and press Undo: its own details come back.

The date taken is a representative date: a month or day you did not give becomes January or the 1st, because EXIF cannot say "only the year". The exact wording is kept next to it.

## Postcards on a white lid

Scan postcards on the white lid (cream, tan and white cards, a few touching the edge of the scan, some with a strongly coloured picture and a bare margin) and add the scan. With **Cut around** set to *The whole card or print* (Detection tab, the default) every box should follow the card's paper edge, margins included, for all three detectors, and the scan should not need hand fixing. Switch it to *Only the picture*, press *Re-detect all* and the boxes go back to the pictures (white borders left out). Known to be hard: a card whose margin is as white as the lid, cards that touch each other, and a card with a plain white strip at the bottom (the strip is not told from the lid). For a corrected project file, `oxicut detect scan.jpg --mode v3 | node tools/compare-project.mjs scan.oxicut` prints how well each hand-fixed box was found.

## Pictures that lie the wrong way

Add a scan where some pictures lie sideways or upside down (the Detection tab switch **Turn pictures the right way up** is on by default). Without the helper file the switch says it needs a one-time download and offers a **Download** button; the Downloads tab lists the same file as "Orientation model". After the download, press *Re-detect all*. Check:

- sideways and upside-down pictures come out upright in the Crops view with a small **Turned** mark beside the size; pictures that were already upright are left alone, and an unclear one (abstract art) is left as it was rather than guessed;
- Left or Right on a turned picture removes its mark; *Undo* on the canvas brings the turn back;
- close the app and open it again: the turns and marks are still there;
- switch the setting off and re-detect: the pictures come out as found, nothing turned.

## Another cut for one picture

Select one picture on the canvas. The **Another cut** button (left column, under Wider) pulses while the app looks for other ways to cut it, then either turns active with a small count or stays grey. Press it: a bar at the top of the canvas names the cut ("Another cut, 1 of 3: cuts wider") and the box on the canvas changes; **Previous** and **Next** step through them. Check:

- **Use this** keeps the shown cut as one undo step (*Undo* brings the old box back); **Cancel**, **Esc** and selecting another picture put the old box back;
- with *Only the picture* chosen under **Cut around**, a postcard's box offers the whole card, and with the whole card chosen it offers the picture only;
- a picture that was turned (by you or automatically) keeps its turn in every suggestion;
- with the full v7 chosen and the models downloaded, the first lookup on a scan takes a few seconds and the next picture on the same scan is quick;
- with "reduce motion" on in Windows, the button does not pulse and the count does not fade in.

## Which picture is which, and when settings changed

- Select a picture on the scan, then press **Crops**: its card has a ring and a small "Selected" mark, and the view opens scrolled to it (also under *All scans*). Open a picture larger, step to another, close, and press **Scan**: that box is selected on the canvas.
- Detect a scan, then change a detection setting (the detector, smallest or largest print, trim, cut around, or *Turn pictures the right way up*): **Re-detect** in the toolbar becomes **Re-detect with new settings** and is outlined, and in the Detection tab the *Re-detect all* button is outlined with a line such as "2 of 2 scans were found with other settings." Pressing either brings the scan back to normal. A change to an export option or the theme does nothing.
- Move a box by hand, then change a setting: that scan's button stays plain. Close and open the project: the outlines come back where a scan was found with other settings; a project from before this was added shows none.

## A newer version

Needs a newer release than the one you run. Without publishing anything, start a small web server in a folder that holds `releases.json` (GitHub's list format: `tag_name`, `name`, `body`, `html_url`, `published_at`, `draft`, `prerelease`) and a `CHANGELOG.md`, and start the app with `OXICUT_UPDATE_URL=http://127.0.0.1:8000`. Check:

- a small arrow appears next to "OxiCut 0.x.y" in the bottom bar and in the About window ("Version N is available." and **See what's new**); click it: the **What is new** window lists the entries newer than yours, newest first (not yours, not *Unreleased*), scrolls when there are many, and **Get version N** opens the release page;
- without a changelog the window shows the release's own notes; with the newest release equal to yours there is no arrow and About says "You have the newest version.";
- Settings → Updates: with the box ticked off (or `OXICUT_NO_UPDATE_CHECK=1`) nothing is asked (the server log shows no request); a server that is down or an answer that makes no sense changes nothing on screen;
- the arrow and the window are reachable and closable by keyboard (Tab, Enter, Esc).

## Fronts and backs

Scan the fronts of some postcards, then turn the stack over and scan the backs. Add both scans, open the **Crops** view on the scan of the fronts and choose **Backs are on another scan…**. Check:

- every front card shows its back beside it, the names read `name-1-front` and `name-1-back`, and the scan list says "fronts" and "backs";
- *Its back* changes the back (a back taken from another card is moved), *Not a pair* saves both on their own again, *Match again* starts over, *Stop pairing* undoes the link;
- *Export this scan* on either scan saves both scans together, into the fronts' `OxiCut output` folder (or the folder chosen in the Export tab), and *Export all approved* leaves a pair out while the other scan is not approved, and says so;
- close the app and open it again: the pairs are still there; delete one crop and press Undo: its pair comes back.

## Automatic tests

Every pull request runs the `web` check (type check, build, tests). The `rust` check (clippy, build, tests) only runs when a Rust-related file changed, so for a change that only touches the interface or the npm packages it shows as skipped, which is fine. Running the workflow by hand from the Actions tab tests everything.

## Known gaps

- v7-lite is only part of ScanSplitter's v7 (no models): it finds the same number of prints as v3. The full v7 is much better at counting them, but a whole album page can still come out as one box, as in ScanSplitter.
- Both detectors can miss pale prints on a white background and can merge prints that touch. These are the same limits ScanSplitter's v3 has, which is where the detector comes from.
- EXIF is copied from JPEG and PNG scans only; from a TIFF scan the crops get the resolution and colour profile, not the EXIF (dates, scanner make).
- Projects are saved, and a scan file that has moved cannot be re-linked from inside the app (move it back, or keep the scans next to the project file).
- Fronts and backs are matched by where the pictures lie (mirrored left to right, or top to bottom, whichever fits better); it cannot read what is on the card. A stack turned over in a different order needs a swap by hand (the *Its back* menu on each card).
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
