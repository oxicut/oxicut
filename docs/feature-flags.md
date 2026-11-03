# Feature flags

A feature flag keeps something that is built but not ready for everyone out of sight. While a flag is off, the app behaves as if the feature did not exist: no tab, no button, no setting, nothing written to the user's files. Nothing about a hidden feature goes in the README, the app, the website or the release notes. The code stays in the repository, tested and documented here.

## Switching a flag on

For one run, set the environment variable before starting the app (names separated by commas, semicolons or spaces, case does not matter):

```
$env:OXICUT_FEATURES = "photo-details"
.\target\release\oxicut-app.exe
```

Or for good, in the settings file (`%APPDATA%\oxicut\settings.json`, or `OXICUT_DATA_DIR` when that is set):

```json
{ "features": ["photo-details"] }
```

There is no switch on screen. A normal installation has no flags on.

## The flags

| Flag | What it hides | Since |
|---|---|---|
| `photo-details` | Date, place, caption and people: the **Details** tab, **About this picture…** on each crop card, the short line on the cards, and writing these into exported pictures (EXIF date taken and description, and an XMP packet). | pull request #3 |

With `photo-details` off, a project that holds details (made by someone who had it on) opens normally and keeps them; they are just not shown and not written to any picture. The engine (`crates/oxicut-core`: `details.rs`, `xmp.rs`) is unchanged: it only writes what it is handed, and the app hands it nothing.

## How it works

- Rust (`app/src-tauri/src/settings.rs`): `parse_features` reads the environment variable and the `features` list of the settings file, and `enabled_features()` returns the names that are on. The `features` command (`lib.rs`) hands them to the screen once, at start.
- The screen (`app/src/store.ts`): `state.features` holds them, `hasFeature("name")` asks, and `FeatureName` lists the known names, so a typo does not compile.
- Hiding is done where the feature meets the user, and where it would reach a file: `v-if` on the interface parts, and a single check in `detailsForExport` so nothing is ever sent to an export (or the larger view's final result) while the flag is off.

## Adding a flag

1. Add the name to `FeatureName` in `store.ts`.
2. Guard the interface parts with `hasFeature("name")`, and any code path that writes files or changes what the user's data means.
3. Add the flag to the table above, and to the roadmap entry of the feature ("built, hidden behind the `name` flag").
4. Test both ways: off (nothing visible, nothing written) and on (`OXICUT_FEATURES`).

Nothing on the Rust side needs a change: any name in the variable or the settings file is passed through.

## Removing a flag

When the feature ships, delete the checks, the `FeatureName` entry and the table row, and say so in `docs/PROGRESS.md`.

## Why there is no library

Widely used flag libraries were looked at (crates.io and npm, November 2026) and none fits an offline desktop app that must not talk to anyone: the popular ones are clients of hosted or self-hosted flag services (LaunchDarkly, Unleash, Flagsmith, GrowthBook), or vendor-neutral layers (OpenFeature) that still need a provider written for them, and the Vue-specific plugins have almost no users. For one switch read from two local places, about 25 lines of our own are simpler and add no dependency. The flag is shaped like OpenFeature's boolean call on purpose, so moving to a library later, if rollouts or per-user targeting are ever needed, would be a small change.
