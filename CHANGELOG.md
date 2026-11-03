# Changelog

What is new in each version of OxiCut, in plain words. The newest version is first.

## [Unreleased]

### Added
- **Fronts and backs.** Scan the fronts of your postcards, turn the stack over and scan the backs. OxiCut matches each front with its back and saves them as a pair, named `-front` and `-back`.
- **Whole cards.** A postcard on a white scanner lid is now cut out whole, margins and all. In the Detection tab, *Cut around* lets you choose *Only the picture* instead.
- **Pictures turned the right way up for you.** Sideways and upside-down pictures are turned while they are found. This needs a one-time download of a helper file (about 80 MB), offered when you switch it on in the Detection tab. Turned pictures say **Turned** in the Crops view, and Left or Right turns one back.
- **Another cut.** Select a picture and press **Another cut** to try other ways the program found to cut it out. A small number on the button tells you when there are some. **Use this** keeps one, **Cancel** puts the old one back.
- **Your selected picture is easy to find in the Crops view.** It has a frame and a **Selected** mark, and the view scrolls to it. Opening a picture larger and going back to the scan selects it there.
- **A hint to detect again.** When you change a setting that changes what is found, **Re-detect** says **Re-detect with new settings** and is outlined, and so is **Re-detect all** in the Detection tab, with a line saying how many scans were found with other settings.
- **A quiet check for a newer version.** OxiCut looks once when it starts. If there is a newer version, a small arrow appears next to the version in the bottom bar and in the About window, and clicking it shows what is new. It only asks GitHub for the newest version number and the list of changes, and nothing about you or your scans is sent. You can switch it off in Settings.
- **Two buttons in the About window:** one for the OxiCut website, one for the project page for developers.

### Fixed
- A tall postcard (turned to portrait) showed only its middle in the Crops view. It is shown whole now.
