import assert from "node:assert/strict";
import { test } from "node:test";
import { assessClose, boxesKey, type GuardScan } from "./closeGuard.ts";

const box = (cx = 100, w = 80) => ({ cx, cy: 50, w, h: 60, angle: 0 });
const detected = boxesKey([box()]);

function scan(status: string, overrides: Partial<GuardScan> = {}): GuardScan {
  return { status, boxes: [box()], detectedKey: detected, exportedKey: null, ...overrides };
}

test("nothing to lose: no scans, or scans nobody touched", () => {
  assert.equal(assessClose([], false).needsConfirm, false);
  assert.equal(assessClose([scan("pending"), scan("flagged"), scan("error", { boxes: [] })], false).needsConfirm, false);
});

test("approved scans that were never exported are counted, but the saved project means closing is fine", () => {
  const result = assessClose([scan("approved"), scan("approved")], false);
  assert.equal(result.neverExported, 2);
  assert.equal(result.needsConfirm, false);
});

test("an approved scan without boxes has nothing to export", () => {
  assert.equal(assessClose([scan("approved", { boxes: [] })], false).neverExported, 0);
});

test("exported and unchanged is safe; exported then edited is not", () => {
  const exported = scan("approved", { exportedKey: detected });
  assert.equal(assessClose([exported], false).neverExported + assessClose([exported], false).changedSinceExport, 0);
  const edited = scan("approved", { boxes: [box(130)], exportedKey: detected });
  const result = assessClose([edited], false);
  assert.equal(result.changedSinceExport, 1);
  assert.equal(result.neverExported, 0);
});

test("edits that were not approved are counted", () => {
  const result = assessClose([scan("pending", { boxes: [box(130)] }), scan("flagged", { boxes: [] })], false);
  assert.equal(result.editedUnapproved, 2); // moving a box, and deleting every box
  assert.equal(result.needsConfirm, false);
});

test("undoing back to the detected boxes, or back to the exported ones, is not an edit", () => {
  assert.equal(assessClose([scan("pending", { boxes: [box(100.04)] })], false).editedUnapproved, 0);
  const exportedKey = boxesKey([box(130)]);
  assert.equal(assessClose([scan("pending", { boxes: [box(130)], exportedKey })], false).editedUnapproved, 0);
});

test("only a running export asks before closing", () => {
  const result = assessClose([], true);
  assert.ok(result.exporting && result.needsConfirm);
  assert.equal(assessClose([scan("approved"), scan("pending", { boxes: [box(130)] })], false).needsConfirm, false);
});

test("boxesKey ignores ids and tiny differences but sees real ones", () => {
  assert.equal(boxesKey([{ ...box(), id: 1 } as never]), boxesKey([{ ...box(100.03), id: 9 } as never]));
  assert.notEqual(boxesKey([box(100)]), boxesKey([box(100.2)]));
  assert.equal(boxesKey([]), "");
});
