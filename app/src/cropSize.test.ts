import assert from "node:assert/strict";
import { test } from "node:test";
import { SMALLEST_SIDE, edgeStep, resizeBox } from "./cropSize.ts";

const scan = { width: 1000, height: 800 };
const box = { cx: 500, cy: 400, w: 400, h: 300, angle: 0 };

test("each click moves every edge by 1% of the shorter side, and at least one pixel", () => {
  assert.equal(edgeStep({ w: 400, h: 300 }), 3);
  assert.equal(edgeStep({ w: 2200, h: 3200 }), 22);
  assert.equal(edgeStep({ w: 40, h: 30 }), 1);
});

test("closer takes the same off every side and keeps the centre and the angle", () => {
  const next = resizeBox({ ...box, angle: 7 }, "closer", scan);
  assert.deepEqual(next, { cx: 500, cy: 400, w: 394, h: 294, angle: 7 });
});

test("wider adds the same to every side", () => {
  assert.deepEqual(resizeBox(box, "wider", scan), { cx: 500, cy: 400, w: 406, h: 306, angle: 0 });
});

test("a crop cannot be cut closer than the smallest side", () => {
  let small = { cx: 500, cy: 400, w: 30, h: 100, angle: 0 };
  assert.ok(resizeBox(small, "closer", scan));
  small = { ...small, w: SMALLEST_SIDE + 1 };
  assert.equal(resizeBox(small, "closer", scan), null);
});

test("wider stops where the crop would reach past the scan, also when it is turned", () => {
  assert.equal(resizeBox({ cx: 500, cy: 400, w: 1000, h: 300, angle: 0 }, "wider", scan), null);
  assert.equal(resizeBox({ cx: 500, cy: 400, w: 400, h: 800, angle: 0 }, "wider", scan), null);
  // Turned a little, the corners reach out further than the sides suggest.
  assert.equal(resizeBox({ cx: 500, cy: 400, w: 990, h: 300, angle: 5 }, "wider", scan), null);
  assert.ok(resizeBox({ cx: 500, cy: 400, w: 900, h: 300, angle: 5 }, "wider", scan));
});

test("closer and then wider gets back to the same box", () => {
  const closer = resizeBox(box, "closer", scan)!;
  const back = resizeBox({ ...closer, w: closer.w, h: closer.h }, "wider", scan)!;
  assert.equal(back.w, 400);
  assert.equal(back.h, 300);
});
