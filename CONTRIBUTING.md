# Contributing to OxiCut

Thank you for wanting to help. OxiCut cuts scanned photos and postcards apart into separate, straightened pictures. It is made for people with a shoebox of prints and a flatbed scanner, not for programmers, and that shapes how it is built.

## Before you start

- **Open an issue first** for anything bigger than a small fix, so we can agree on it before you spend time. Small fixes and typos can go straight to a pull request.
- Look at the [open issues](https://github.com/oxicut/oxicut/issues); some are a good place to start.

## The rules of the house

- **Plain words.** Everything a person can read or click is written for someone who does not care how it works: short sentences, no jargon (no "ONNX", "checksum", "EXIF"), no numbers nobody needs, and errors that say what to do next. Name things in full ("the **Careful (v7)** detection method needs ...", not "Careful needs ..."). The full rules are in [`CLAUDE.md`](CLAUDE.md).
- **Windows first.** No dependency that needs a C or C++ build without asking first. Models are downloaded on first use, never bundled.
- **Be careful with the network.** The program uses it only for the downloads a person starts and the quiet check for a newer version. Talk to us in an issue before adding anything else.
- **No em dashes** in text or code comments.

## Making a change

1. Make a branch from `main`, one change per pull request.
2. Run the checks:
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
   - in `app`: `npm test` and `npm run build`
3. Add a line under *Unreleased* in [`CHANGELOG.md`](CHANGELOG.md) if people will notice the change (see [`docs/changelog.md`](docs/changelog.md)), and update [`docs/testing.md`](docs/testing.md) if there is something new to try by hand.
4. Open a pull request. The template has a short checklist.

To build and try the app: `cd app; cargo tauri build --no-bundle`, then run `target\release\oxicut-app.exe` (close a running copy first, it locks the file).

## Licence

OxiCut is under the [GNU General Public License v3.0](LICENSE). By contributing you agree that your contribution is under the same licence.
