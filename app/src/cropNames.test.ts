import assert from "node:assert/strict";
import { test } from "node:test";
import { baseName, cropFileName, expandPattern, outputExtension, sanitizeFileName } from "./cropNames.ts";
import { normalizeAngle, turnBox } from "./cropTurn.ts";

// The cases below are the ones `export.rs` tests, so the screen and the exporter agree on names.
test("characters Windows refuses become underscores, and trailing dots and spaces go", () => {
  assert.equal(sanitizeFileName('a<b>c:d"e/f\\g|h?i*j'), "a_b_c_d_e_f_g_h_i_j");
  assert.equal(sanitizeFileName("holiday. "), "holiday");
  assert.equal(sanitizeFileName("  .. name .. "), "name");
});

test("reserved device names and empty names are made safe", () => {
  assert.equal(sanitizeFileName("CON"), "_CON");
  assert.equal(sanitizeFileName("lpt1.txt"), "_lpt1.txt");
  assert.equal(sanitizeFileName(""), "scan");
  assert.equal(sanitizeFileName("..."), "scan");
});

test("very long names are cut at 150 characters", () => {
  assert.equal(Array.from(sanitizeFileName("x".repeat(400))).length, 150);
});

test("the pattern gets the scan name and the print number, and always a number", () => {
  assert.equal(expandPattern("{source}-{n}", "album-07", 3), "album-07-3");
  assert.equal(expandPattern("print_{n}", "a", 1), "print_1");
  assert.equal(expandPattern("{source}", "a", 2), "a-2");
  assert.equal(expandPattern("   ", "a", 4), "a-4");
});

test("the extension follows the chosen format, or the scan's own", () => {
  assert.equal(outputExtension("same", "a.JPEG"), "jpg");
  assert.equal(outputExtension("same", "a.tiff"), "tif");
  assert.equal(outputExtension("same", "a.bmp"), "png");
  assert.equal(outputExtension("same", "noextension"), "png");
  assert.equal(outputExtension("tiff", "a.jpg"), "tif");
  assert.equal(outputExtension("jpg", "a.png"), "jpg");
});

test("a whole file name, and the last part of a path", () => {
  assert.equal(cropFileName("{source}-{n}", "album-07.png", 3, "same"), "album-07-3.png");
  assert.equal(cropFileName("{source}-{n}", "a.b.tif", 1, "jpg"), "a.b-1.jpg");
  assert.equal(baseName("C:\\scans\\out\\a-1.png"), "a-1.png");
  assert.equal(baseName("D:/x/y.tif"), "y.tif");
});

test("turning a box swaps its sides and moves the angle by a quarter turn", () => {
  const box = { cx: 10, cy: 20, w: 300, h: 200, angle: 5 };
  assert.deepEqual(turnBox(box, "right"), { cx: 10, cy: 20, w: 200, h: 300, angle: -85 });
  assert.deepEqual(turnBox(box, "left"), { cx: 10, cy: 20, w: 200, h: 300, angle: 95 });
  // Four turns bring the box back.
  let again = box;
  for (let i = 0; i < 4; i++) again = turnBox(again, "right");
  assert.deepEqual(again, box);
});

test("angles stay in [-180, 180)", () => {
  assert.equal(normalizeAngle(190), -170);
  assert.equal(normalizeAngle(-190), 170);
  assert.equal(normalizeAngle(180), -180);
  assert.equal(normalizeAngle(0), 0);
  assert.equal(normalizeAngle(720), 0);
  assert.equal(turnBox({ cx: 0, cy: 0, w: 1, h: 2, angle: -170 }, "right").angle, 100);
});
