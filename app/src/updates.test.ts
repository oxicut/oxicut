import assert from "node:assert/strict";
import { test } from "node:test";
import { compareVersions, entriesSince, entryFor, isNewer, parseChangelog, pickNewest, shownVersion, type Release } from "./updates.ts";

test("versions are compared by number, not as text", () => {
  assert.equal(isNewer("0.1.0", "0.0.9"), true);
  assert.equal(isNewer("0.10.0", "0.9.0"), true);
  assert.equal(isNewer("v1.0.0", "0.99.99"), true);
  assert.equal(isNewer("0.1.0", "0.1.0"), false);
  assert.equal(isNewer("0.0.9", "0.1.0"), false);
  assert.equal(compareVersions("v0.1.0", "0.1.0"), 0);
  assert.equal(compareVersions("0.1", "0.1.0"), 0);
});

test("a pre-release is older than the same version, and its parts are compared", () => {
  assert.equal(isNewer("0.1.0", "0.1.0-beta.2"), true);
  assert.equal(isNewer("0.1.0-beta.2", "0.1.0"), false);
  assert.equal(isNewer("0.1.0-beta.10", "0.1.0-beta.9"), true);
  assert.equal(isNewer("0.1.0-rc.1", "0.1.0-beta.9"), true);
  assert.equal(isNewer("0.2.0-beta.1", "0.1.0"), true);
});

test("something that is not a version is never newer", () => {
  assert.equal(isNewer("nightly", "0.1.0"), false);
  assert.equal(isNewer("0.1.0", "nightly"), false);
  assert.equal(compareVersions("", "0.1.0"), null);
});

const release = (version: string, extra: Partial<Release> = {}): Release => ({
  version,
  name: `OxiCut ${version}`,
  notes: "",
  url: `https://github.com/oxicut/oxicut/releases/tag/${version}`,
  publishedAt: "",
  prerelease: true,
  ...extra,
});

test("the newest release is the highest version, pre-release or not", () => {
  const newest = pickNewest([release("v0.0.1"), release("v0.2.0"), release("v0.1.0", { prerelease: false }), release("nightly")]);
  assert.equal(newest?.version, "v0.2.0");
  assert.equal(pickNewest([]), null);
  assert.equal(pickNewest([release("nightly")]), null);
  assert.equal(shownVersion("v0.2.0"), "0.2.0");
});

const CHANGELOG = `# Changelog

Notes before the first version are ignored.

## [Unreleased]

### Added
- Not out yet.

## [0.3.0] - 2027-02-01

### Fixed
- Third.

## [0.2.0] - 2027-01-01

- Second.

## 0.1.0 – 2026-12-01
- First.

## [0.0.1] - 2026-10-30
- Test version.
`;

test("a changelog is read version by version, and Unreleased is left out", () => {
  const entries = parseChangelog(CHANGELOG);
  assert.deepEqual(entries.map((e) => [e.version, e.date]), [
    ["0.3.0", "2027-02-01"],
    ["0.2.0", "2027-01-01"],
    ["0.1.0", "2026-12-01"],
    ["0.0.1", "2026-10-30"],
  ]);
  assert.equal(entries[0].body, "### Fixed\n- Third.");
  assert.equal(entries[2].body, "- First.");
  assert.deepEqual(parseChangelog("nothing here"), []);
  assert.deepEqual(parseChangelog(""), []);
});

test("the entries since a build are the newer ones up to the newest, newest first", () => {
  const entries = parseChangelog(CHANGELOG);
  assert.deepEqual(entriesSince(entries, "0.0.1", "0.3.0").map((e) => e.version), ["0.3.0", "0.2.0", "0.1.0"]);
  assert.deepEqual(entriesSince(entries, "0.0.1", "0.2.0").map((e) => e.version), ["0.2.0", "0.1.0"], "nothing past the newest release");
  assert.deepEqual(entriesSince(entries, "0.3.0", "0.3.0"), []);
  assert.equal(entryFor(entries, "0.2.0")?.body, "- Second.");
  assert.equal(entryFor(entries, "9.9.9"), null);
});
