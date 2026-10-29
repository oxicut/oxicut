# OxiCut

A native Windows app (Tauri 2, Vue 3, Rust engine) that splits flatbed scans of photo prints and postcards into separate, straightened images. A Rust port of ScanSplitter; see `README.md` for the story, `roadmap.md` for what is done and next, `docs/` for notes.

## Who it is for

Not nerds. Think of someone in middle age or older with a shoebox of prints and a flatbed scanner, who wants the pictures sorted and does not care how it works. This applies to everything the user can read or click:

- **Plain words.** No jargon (no "ONNX", "checksum", "angle", "EXIF", "scratch folder", "cache"). Say what happened and what to do next, in short sentences.
- **No numbers nobody needs.** Do not show how long something took, byte counts, hashes or internal ids. Show what helps a decision: how many pictures, where they went, what is wrong.
- **Errors are for people.** Explain the problem and the next step; keep OS error codes and raw messages out of the main text.
- **Reassure where it matters.** Originals are never changed, work is saved automatically, nothing is deleted without asking.
- Large click targets, clear labels (words over icons), no surprises.

## Working here

- No em dashes in text or code comments.
- Windows first. No OpenCV; ask before adding a dependency that needs a C or C++ build. Models are downloaded on first use, never bundled.
- Detection mirrors ScanSplitter (`reference/ScanSplitter`): when something looks wrong, check whether the Python original does the same and say so.
- Tests: `cargo test --workspace`, `cargo clippy --workspace --all-targets`, and in `app`: `npm test`, `npm run build`. Build the app with `cd app; cargo tauri build --no-bundle` (close the running app first, it locks the exe).
- Edit files with the editing tools, not PowerShell `Set-Content` (it breaks UTF-8) and not shell one-liners with backslashes (they get collapsed).
- Keep `docs/TESTING.md`, `docs/PROGRESS.md` and `roadmap.md` in step with what changes. Commit only when asked.
