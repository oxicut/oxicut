# Before a release

For 0.1.0 and the ones after. Tick things off in the release pull request, or write what was skipped and why.

## What the user can see

- [ ] **Audit the screen text.** Nothing may promise something that is not there. Search the interface and the engine's messages for leftovers and read every hit:
  `grep -rniE "todo|fixme|not implemented|coming soon|not yet|planned|work in progress|later phase|stub|placeholder|lorem" app/src app/index.html --include=*.vue --include=*.ts --include=*.html` and the same over `app/src-tauri/src` (user-facing strings only; comments are fine). Then open every window, tab and dialog in the built app once (Detection, Details if it is switched on, Export and Models tabs, Settings, About, What is new, Welcome, Projects, Close) and look for anything unfinished, empty, or in a developer's words.
  - Last run (2026-11-03): no leftovers in the interface text. Found instead, to decide before 0.1: the detector names *v3*, *v7-lite* and *v7*, the words *MobileSAM*, *checksum*, *EXIF* and *DPI* in the Detection, Models and Export tabs and in the welcome dialog. They were in the plain-words rule's way ("no jargon"). Fixed 2026-11-03: the detection methods are Basic (v3), Standard (v7-lite) and Careful (v7), the Models tab is Downloads, and the Export tab and the welcome dialog say it in plain words. Run the search again before the release, and look at every window once.
- [ ] **Features that are not ready stay hidden.** `docs/feature-flags.md`: nothing of a flagged feature in the app, the README or the website while it is off.
- [ ] **Accessibility pass.** The main flow by keyboard alone, a screen reader on the main windows, contrast in light and dark, no colour-only signals, reduced motion respected.

## The release itself

- [ ] **Changelog** has a section for the version, in plain words, with the date (`docs/changelog.md`). The workflow refuses to run without it.
- [ ] **Version** bumped in the workspace `Cargo.toml` (and `Cargo.lock`); the bottom bar and About show it after `cd app; cargo tauri build --no-bundle`.
- [ ] **Everything passes** on `main`: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, in `app`: `npm test`, `npm run build`.
- [ ] **Clean machine.** On a Windows 10 or 11 machine without development tools: install, first start (welcome dialog, model downloads, and what happens with no internet), open a `.oxicut` file by double-clicking, upgrade over the previous version, uninstall. _(Done once: a fresh Windows 10 machine with an i7-7700K installed it and it worked. Not yet tried: Windows 11, no WebView2, upgrade over an old version, uninstall, the double-click.)_
- [ ] **Failure behaviour.** A full disk, a read-only output folder, a corrupt or very large scan, a scan that was moved, a scans folder with hundreds of files; the messages are plain.
- [ ] **Memory.** `docs/memory-usage.md`: the numbers for the machine sizes the website names.
- [ ] **Licence notices.** Third-party licences (Rust and npm packages, the models) come with the installer (`app/src-tauri/resources/third-party-notices.txt`, rebuilt with `node tools/make-notices.mjs` whenever `Cargo.lock` or `app/package-lock.json` changes; the release workflow checks it is current) and open from the About window; the README credits are right. _(Done 2026-11-03: the file, the About link and the release check. Look at the About link in the installed program once.)_
- [ ] **Website** (`oxicut.github.io`) matches the README and the changelog: download button, three-step guide, FAQ, privacy page (what the app asks the network, and when), support contact, and the "Windows protected your PC" explanation while the installer is unsigned.
- [x] **Repository.** Description and homepage set, `SECURITY.md`, `CONTRIBUTING.md`, issue templates. _(Done 2026-11-03; the homepage becomes the website's address once the site has its content.)_
- [ ] Run the Release workflow by hand (tick the test-version box while the project is still in testing), check the release page, the installer name `OxiCut-setup.exe`, the checksums and the notes.
- [ ] Afterwards: a scratch build whose version is lowered by hand sees the new release (the arrow next to the version) and shows its notes.
