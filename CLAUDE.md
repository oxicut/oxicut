# OxiCut

A native Windows app (Tauri 2, Vue 3, Rust engine) that splits flatbed scans of photo prints and postcards into separate, straightened images. A Rust port of ScanSplitter; see `README.md` for the story, `CHANGELOG.md` for what is new, the issues on GitHub for what is next, `docs/` for notes.

## Who it is for

Not nerds. Think of someone in middle age or older with a shoebox of prints and a flatbed scanner, who wants the pictures sorted and does not care how it works. This applies to everything the user can read or click:

- **Plain words.** No jargon (no "ONNX", "checksum", "angle", "EXIF", "scratch folder", "cache"). Say what happened and what to do next, in short sentences.
- **No numbers nobody needs.** Do not show how long something took, byte counts, hashes or internal ids. Show what helps a decision: how many pictures, where they went, what is wrong.
- **Name things in full.** Never write a bare name as if the reader knew it, such as "Careful needs ..." or "Standard works without ...". Say what kind of thing it is, with its full name: "The **Careful (v7)** detection method needs ...", "the **Downloads** tab", "the **Another cut** button". The same goes for the changelog and the docs a user may read.
- **Errors are for people.** Explain the problem and the next step; keep OS error codes and raw messages out of the main text.
- **Reassure where it matters.** Originals are never changed, work is saved automatically, nothing is deleted without asking.
- Large click targets, clear labels (words over icons), no surprises.

## Working here

- No em dashes in text or code comments.
- Windows first. No OpenCV; ask before adding a dependency that needs a C or C++ build. Models are downloaded on first use, never bundled.
- Detection mirrors ScanSplitter (`reference/ScanSplitter`): when something looks wrong, check whether the Python original does the same and say so.
- Tests: `cargo test --workspace`, `cargo clippy --workspace --all-targets`, and in `app`: `npm test`, `npm run build`. Build the app with `cd app; cargo tauri build --no-bundle` (close the running app first, it locks the exe).
- Edit files with the editing tools, not PowerShell `Set-Content` (it breaks UTF-8) and not shell one-liners with backslashes (they get collapsed).
- Features that are built but not ready for everyone hide behind a flag, off by default (`docs/feature-flags.md`); nothing of them shows in the README, the app or the website while it is off.
- Keep `docs/testing.md` in step with what changes. What is left to do lives in GitHub issues, not in a file. Commit only when asked.
- Keep `CHANGELOG.md` in step (`docs/changelog.md`): every pull request that changes what people see or can do adds a line under *Unreleased*, in plain words like the rest of the app. A release is not made until the changelog has a section for its version, and `docs/release-checklist.md` is worked through first (it includes a check that nothing unfinished or "not yet" faces the user).
