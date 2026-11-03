# The changelog

`CHANGELOG.md` in the repository root says what is new in each version, for the people who use OxiCut. The app reads it too: the **What is new** window (opened from the arrow that appears next to the version when a newer one exists, and from the About window) shows the entries newer than the version someone is running, so it has to stay in the format below.

## Format

Keep a Changelog style. One section per version, newest first, under a `## [Unreleased]` section that collects what is coming:

```
## [Unreleased]

### Added
- ...

## [0.1.0] - 2026-11-15

### Added
- ...
### Changed
- ...
### Fixed
- ...
```

- The heading is `## [x.y.z] - yyyy-mm-dd` (the brackets are optional, a plain `## x.y.z - date` works). Anything before the first version heading, and the *Unreleased* section, is not shown in the app.
- `### Added`, `### Changed`, `### Fixed` as needed. Bullets only, one per thing a person would notice.
- Inside a bullet: `**bold**`, `` `code` `` and `[text](https://github.com/oxicut/...)` links work; links to anywhere but the project's own pages show as plain text. Nothing else is interpreted, and no HTML.

## Tone

The same as the app: plain words, short sentences, what happened and what it means for the person. No jargon (no "ONNX", "cache", "checksum", "angle"), no timings or sizes nobody needs (a download size they should know about is fine), no issue or pull request numbers. Nothing about features that are still switched off behind a flag (see `docs/feature-flags.md`), and nothing about changes only a developer would see (tests, workflows, refactoring).

## Keeping it current

- Every pull request that changes what people see or can do adds a line under *Unreleased*. The pull request template has a box for it.
- At release time: bump the version (one place, the workspace `Cargo.toml`), rename *Unreleased* to `[x.y.z] - <date>`, and add a new empty *Unreleased* above it.
- The release workflow stops, with a message, when the changelog has no notes for the version it is building (`tools/changelog-section.sh`), and uses that section as the release notes on GitHub.
