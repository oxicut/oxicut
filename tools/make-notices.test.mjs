// Run with: node --test tools/
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { groupByText, licenceFiles, tidy } from "./make-notices.mjs";

test("the same licence in two places counts as one text, whatever the line endings", () => {
  assert.equal(tidy("MIT License\r\n\r\nCopyright (c) A  \r\n"), "MIT License\n\nCopyright (c) A");
  assert.equal(tidy("  \n text \n\n"), "text");
});

test("packages with the same text are grouped, biggest group first", () => {
  const mit = tidy("MIT License\n\nCopyright (c) 2020 Someone");
  const other = tidy("Apache License");
  const groups = groupByText([
    { name: "a", version: "1", texts: [mit] },
    { name: "b", version: "2", texts: [other, mit] },
    { name: "c", version: "3", texts: [mit] },
  ]);
  assert.equal(groups.length, 2);
  assert.deepEqual(groups[0].packages, ["a 1", "b 2", "c 3"]);
  assert.equal(groups[0].text, mit);
  assert.deepEqual(groups[1].packages, ["b 2"]);
});

test("a package with two licence files lists both, and one listed twice is counted once", () => {
  const groups = groupByText([{ name: "x", version: "1", texts: ["MIT text", "Apache text", "MIT text"] }]);
  assert.equal(groups.length, 2);
  assert.ok(groups.every((g) => g.packages.length === 1));
});

test("licence and notice files are found by name, source files are not", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "notices-"));
  for (const name of ["LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "COPYING.txt", "NOTICE", "license.md", "licensing.rs", "README.md", "license.json"]) {
    fs.writeFileSync(path.join(dir, name), "x");
  }
  fs.mkdirSync(path.join(dir, "licenses"));
  fs.writeFileSync(path.join(dir, "licenses", "LICENSE-FOO"), "x");
  // Compared without capitals: Windows treats `licenses` and `LICENSES` as one folder, and a file is listed once.
  const names = licenceFiles(dir).map((f) => path.relative(dir, f).replace(/\\/g, "/").toLowerCase());
  assert.deepEqual(names, ["copying.txt", "license", "license-apache", "license-mit", "licenses/license-foo", "notice", "license.md"]);
  fs.rmSync(dir, { recursive: true });
});
