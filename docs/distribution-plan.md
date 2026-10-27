# Distributing OxiCut to non-technical, mostly older users

Written 2026-10-27. Nothing here is built yet; the steps are tracked in Phase 7 of [`roadmap.md`](../roadmap.md).

## Context

The audience is not developers: middle-aged and older people with a shoebox of prints and a flatbed scanner. For them a "Windows protected your PC" (SmartScreen) screen on an unsigned installer is a stop sign, and "download from GitHub Releases" is unfamiliar. So: a plain website with one obvious download button, and the Microsoft Store as the primary, trusted install route, with a direct installer as the second.

## Research findings

Found by web search in October 2026. Where the official page could not be read directly, the finding rests on secondary sources and says so.

- **The Store is free to publish to.** Individual developer registration has had no fee since September 2025 ([Windows Developer Blog](https://blogs.windows.com/windowsdeveloper/2025/09/10/free-developer-registration-for-individual-developers-on-microsoft-store/), [Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/publish/whats-new-individual-developer)). Company accounts are reported free too, but that is less consistently documented. Identity verification (a government ID and a selfie) and certification still apply.
- **The Store and signing.** An **MSIX** package needs no purchased certificate: Microsoft re-signs it after certification. An **EXE or MSI** submitted to the Store must already be signed by a CA in Microsoft's trusted root programme (self-signed is rejected) and be hosted at a versioned URL, with updates handled by the app itself. So MSIX is the path that costs nothing. See [code signing options](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options) and [choose a distribution path](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/choose-distribution-path).
- **Tauri does not make MSIX.** `cargo tauri build` produces NSIS `setup.exe` and WiX `.msi` only; [Tauri's Microsoft Store page](https://v2.tauri.app/distribute/microsoft-store/) says the same. Microsoft's [`winapp` CLI guide for Tauri](https://learn.microsoft.com/en-us/windows/apps/dev-tools/winapp-cli/guides/tauri) packages the built exe into MSIX (`winapp pack`, which needs a `Package.appxmanifest` and an `Assets` folder). The package's Name and Publisher must match what Partner Center reserved exactly. A desktop (full trust) app will also have to declare the `runFullTrust` capability at submission.
- **A direct installer needs a certificate to avoid SmartScreen.**
  - Free route: [SignPath Foundation](https://signpath.org), for open-source projects. Reported requirements are an OSI-approved licence, a public repository, CI-built releases and some verifiable reputation, and applications can be slow. These details come from other projects' notes; signpath.org itself could not be read during the research, so check its terms before applying.
  - Paid route: Azure Artifact Signing (formerly Trusted Signing), about $10 a month. Eligibility is restricted (organisations in the US, Canada, EU and UK, individuals in the US and Canada, per Microsoft's table) and has been described inconsistently; see this [Microsoft Q&A thread](https://learn.microsoft.com/en-us/answers/questions/5977141/azure-artifact-signing-trusted-signing-is-a-us-llc).
- **`tauri-action`** ([repository](https://github.com/tauri-apps/tauri-action), [Tauri's GitHub pipeline guide](https://v2.tauri.app/distribute/pipelines/github/)) builds and publishes to a GitHub Release. It only writes the updater's `latest.json` when updater signature files exist, and wants `tagName` set so the updater URLs are right.
- **CI gotchas.** A tag or release created with the default `GITHUB_TOKEN` does not trigger other workflows, so "bump, tag, then a second workflow builds" does not work without a personal access token; do it in one workflow. [release-please](https://github.com/googleapis/release-please-action) cannot rewrite a workspace-inherited `version.workspace = true`, so it is a poor fit for this Cargo workspace.
- **winget** can be fed automatically: [Komac](https://github.com/russellbanks/Komac) or [wingetcreate](https://github.com/microsoft/winget-create) open the pull request (a classic token with `public_repo` is needed), see [Microsoft's submission guide](https://learn.microsoft.com/en-us/windows/package-manager/package/repository). Useful for technical users only, so later.

## Recommendation

1. **Microsoft Store: the main route for the target audience.** One click to install, automatic updates, no SmartScreen warning, no certificate to buy.
2. **A website with a direct installer: the second route**, for people who will not use the Store. An NSIS `setup.exe`, installing per user (no administrator prompt). Until it is signed, the page shows a short, friendly "if Windows shows a blue screen, click More info, then Run anyway" with a screenshot. Apply to SignPath once the repository is public and has releases.
3. **winget** later, optional.

**The website** (GitHub Pages, free; a custom domain is optional, about 10 euro a year): plain language and large type; one big "Get it from Microsoft Store" button with a smaller "Download for Windows (installer)" below it; a three-step "how it works" with screenshots (Add, Check, Export); the Windows 10/11 requirement; a short FAQ (where do my pictures go, is my original changed, what is the model download); a privacy page (nothing leaves the computer except the one-time model download); and a support contact. The Store listing also needs screenshots, a description, an age rating and a privacy-policy URL, which the website provides.

## Build steps

1. **One source of truth for the version.** The workspace `version` in `Cargo.toml`. `oxicut_core::VERSION` becomes `env!("CARGO_PKG_VERSION")` (replacing the hardcoded `0.0.1`); `tauri.conf.json` and `app/package.json` follow it (leave the version out of `tauri.conf.json` so Tauri uses the Cargo one; verify this). The Store needs four-part versions ending in `.0`.
2. **Installer settings** in `app/src-tauri/tauri.conf.json`: publisher, copyright, NSIS per-user mode, WebView2 handling for older Windows 10, icons (already present). Build `nsis` only on CI; skip WiX unless someone asks for an `.msi`.
3. **Release workflow** (`.github/workflows/release.yml`), one workflow, started by a manual "bump" run (patch, minor or major): bump the versions with `cargo set-version`, commit and tag, then build on `windows-latest` (Rust cache, `cargo test`, `npm test`) and publish a GitHub Release with the NSIS installer under a stable file name (so `releases/latest/download/OxiCut-setup.exe` always works for the website button), the command-line zip, and checksums. It also runs on a pushed `v*` tag for manual releases.
4. **MSIX job**, added once the name is reserved in Partner Center: package the exe with `winapp pack` using the reserved identity (kept as repository variables) and upload the unsigned MSIX as an artifact for submission. Submitting to the Store stays manual at first.
5. **Website** in a `website` folder published with GitHub Pages: the pages above, the Store badge, the direct download link.
6. **Signing** with SignPath once eligible; then winget.

## What only the project owner can do

Make the GitHub repository public under an `oxicut` organisation or user, register as a Store developer (ID and selfie) and reserve the name "OxiCut", choose a support email, decide on a domain, and approve a privacy text.

## Verification

- Release workflow: a dry run with `workflow_dispatch` to a draft release; install the produced `setup.exe` on a clean Windows machine or VM (no Rust, no assumptions about WebView2), run the app on the sample scans, and uninstall cleanly.
- The `latest/download` URL returns the installer; the website renders on a phone and with a large-text setting.
- The MSIX installs with `Add-AppxPackage` on a test machine (self-signed for the test) and passes the Windows App Certification Kit before submission.
- `cargo test --workspace` and `npm test` stay green in CI.
