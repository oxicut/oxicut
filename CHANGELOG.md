# Changelog

What is new in each version of OxiCut, in plain words. The newest version is first.

## [Unreleased]

The first full version. OxiCut cuts a scan of several photos or postcards apart into separate, straightened pictures. It started from the ideas of ScanSplitter and has grown into a desktop program of its own.

### Added

**Detection: finding the pictures on a scan**
- **Three detection methods:** **Basic**, **Standard** (the default; it also trims white borders so the box sits on the photograph) and **Careful** (the most accurate, and the best at counting prints). Careful needs some extra files, about 45 MB, downloaded once. OxiCut offers them the first time you start it, and in the **Downloads** tab. The technical names (v3, v7-lite and v7) stay in brackets.
- **Whole postcards.** A card on a white scanner lid is cut out whole, margins and all. In the Detection tab, *Cut around* lets you choose *Only the picture* instead.
- **Settings for what counts as a picture:** the smallest and largest print, and how much to trim from each side.
- **Pictures turned the right way up for you.** Sideways and upside-down pictures are turned during detection. This needs a one-time download of a helper file (about 80 MB), offered when you switch it on in the Detection tab. Turned pictures say **Turned** in the Crops view, and Left or Right turns one back.
- **Detection runs in the background,** several scans at a time, while you work. Scans where nothing looks wrong are approved by themselves. The ones that need a look wait for you, with a plain note saying why (nothing found, touching the edge of the scan, an unusual shape, overlapping boxes).
- **Add scans** one by one, a whole folder at a time, or by dropping them on the window. JPEG, PNG and TIFF, including 16-bit TIFF.

**Checking and fixing**
- **A frame around every picture.** Move it, resize it, turn it (hold Shift to snap to level), add one that was missed, split two pictures that were found as one, delete one that is not a picture, or press **Straighten**.
- **Closer and Wider** cut a picture a little tighter or looser, for example to lose a white border.
- **Magnifiers.** A magnifier follows the handle you drag, and while you move or turn a frame, all four of its corners are magnified so you can line them up.
- **Undo and redo** for every scan, so you can try things freely.
- **Zoom and move around the scan** with the mouse wheel and by dragging the background; **Fit** brings the whole scan back.
- **Another cut.** Select a picture and press **Another cut** to try other ways the program found to cut it out. A small number on the button tells you when there are some. **Use this** keeps one, **Cancel** puts the old one back.
- **Keyboard:** Enter approves a scan and moves on, the arrow keys go to the next and previous scan, Delete removes a frame, and Ctrl+Z and Ctrl+Y undo and redo.
- **A hint to detect again.** When you change a setting that changes the detection, **Re-detect** says **Re-detect with new settings** and is outlined, and so is **Re-detect all** in the Detection tab, with a line saying how many scans were detected with other settings.

**The finished pictures**
- **The Crops view** shows every straightened picture with the name it will get. Turn one left or right, remove it, look at it larger, or export a scan from there. In the larger view you can cut closer or wider and check the final result before you save.
- **Your selected picture is easy to find.** It has a frame and a **Selected** mark in the Crops view, and the view scrolls to it. Opening a picture larger and going back to the scan selects it there.
- **Fronts and backs.** Scan the fronts of your postcards, turn the stack over and scan the backs. OxiCut matches each front with its back, you can change any pair, and the pair is saved together, named `-front` and `-back`.

**Saving your work**
- **Projects.** Your work is saved by itself, a second after every change, in a `.oxicut` file in the OxiCut folder in Documents. Close the window any time and OxiCut opens where you stopped. Keep several projects, rename them, switch between them, and open one by double-clicking it. A project finds its scans again if you move the folder.
- **A warning before you close** with work that is not exported or not approved yet.
- **Export** as the same format as the scan, JPG, PNG or TIFF, with your choice of picture quality, file name pattern and folder. A 16-bit scan stays 16-bit. Your originals are never changed, and an existing file is never overwritten: a new one gets "(2)" in its name.
- **Your scan's details come along:** resolution, colour profile and photo details such as the date taken. You can also have OxiCut note in each picture that it made it.

**The program**
- **Looks the way you like:** follow Windows, light or dark. The scan list and the side panel fold away to make room, and still show the key facts (Ctrl+B and Ctrl+Shift+B).
- **A quiet check for a newer version.** OxiCut looks once when it starts. If there is a newer version, a small arrow appears next to the version in the bottom bar and in the About window, and clicking it shows what is new. It only asks GitHub for the newest version number and the list of changes, and nothing about you or your scans is sent. You can switch it off in Settings.
- **An About window** with two buttons: the OxiCut website, and the project page for developers.
- **Works without internet.** The only things that use it are the downloads you start and the check for a newer version.

### Fixed
- A tall postcard (turned to portrait) showed only its middle in the Crops view. It is shown whole now.
