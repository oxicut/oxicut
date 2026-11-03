import assert from "node:assert/strict";
import { test } from "node:test";
import { detectKey, isOutOfDate, type StaleCheck } from "./detectStale.ts";

const settings = { mode: "v7-lite", minAreaPercent: 2, maxAreaPercent: 80, insetPx: 0, cutTo: "card", autoOrient: true } as const;

test("every setting that changes what is found changes the key", () => {
  const base = detectKey(settings);
  assert.notEqual(detectKey({ ...settings, mode: "v7" }), base);
  assert.notEqual(detectKey({ ...settings, minAreaPercent: 3 }), base);
  assert.notEqual(detectKey({ ...settings, maxAreaPercent: 70 }), base);
  assert.notEqual(detectKey({ ...settings, insetPx: 5 }), base);
  assert.notEqual(detectKey({ ...settings, cutTo: "picture" }), base);
  assert.notEqual(detectKey({ ...settings, autoOrient: false }), base);
  assert.equal(detectKey({ ...settings }), base);
});

test("other things on a settings object do not change the key", () => {
  const more = { ...settings, format: "jpg", theme: "dark", features: ["x"] } as never;
  assert.equal(detectKey(more), detectKey(settings));
});

const found: StaleCheck = { status: "approved", detectedWith: "old", detectedKey: "a;b", boxesKey: "a;b" };

test("a scan found with other settings is out of date", () => {
  assert.equal(isOutOfDate(found, "new"), true);
  assert.equal(isOutOfDate({ ...found, status: "flagged" }, "new"), true);
  assert.equal(isOutOfDate({ ...found, status: "pending" }, "new"), true);
});

test("a scan found with the same settings, or from before this was remembered, is not", () => {
  assert.equal(isOutOfDate(found, "old"), false);
  assert.equal(isOutOfDate({ ...found, detectedWith: "" }, "new"), false);
});

test("a scan being found, or one that failed, is not", () => {
  for (const status of ["queued", "detecting", "error"]) assert.equal(isOutOfDate({ ...found, status }, "new"), false);
});

test("a scan the person has already fixed by hand is not", () => {
  assert.equal(isOutOfDate({ ...found, boxesKey: "a;b;c" }, "new"), false);
});
