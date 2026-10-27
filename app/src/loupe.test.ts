import assert from "node:assert/strict";
import { test } from "node:test";
import { boxCorners, cornerPlaces, handlePoint, sourceSquare } from "./loupe.ts";

const near = (a: number, b: number) => assert.ok(Math.abs(a - b) < 1e-9, `${a} vs ${b}`);

test("an upright box has its corners where they should be, top-left first", () => {
  const corners = boxCorners({ cx: 100, cy: 50, w: 40, h: 20, angle: 0 });
  assert.deepEqual(
    corners.map((c) => [Math.round(c.x), Math.round(c.y)]),
    [
      [80, 40],
      [120, 40],
      [120, 60],
      [80, 60],
    ],
  );
});

test("a quarter turn clockwise moves the top-left corner to the top-right", () => {
  const [first] = boxCorners({ cx: 0, cy: 0, w: 40, h: 20, angle: 90 });
  near(first.x, 10);
  near(first.y, -20);
});

test("a handle sits on its side or corner, and the rotation handle is not a resize handle", () => {
  const box = { cx: 10, cy: 10, w: 20, h: 10, angle: 0 };
  assert.deepEqual(handlePoint(box, "mr"), { x: 20, y: 10 });
  assert.deepEqual(handlePoint(box, "tl"), { x: 0, y: 5 });
  assert.equal(handlePoint(box, "mtr"), null);
});

test("the loupe's square is centred on the point and shrinks as the magnification grows", () => {
  const square = sourceSquare({ x: 50, y: 60 }, 100, 4);
  assert.deepEqual(square, { x: 37.5, y: 47.5, side: 25 });
});

test("an upright box puts its corners in the matching stage corners", () => {
  const places = cornerPlaces(boxCorners({ cx: 100, cy: 100, w: 60, h: 40, angle: 0 }));
  assert.deepEqual(places, ["top-left", "top-right", "bottom-right", "bottom-left"]);
});

test("turning the box moves each corner's loupe with it", () => {
  const places = cornerPlaces(boxCorners({ cx: 100, cy: 100, w: 60, h: 40, angle: 90 }));
  // The box's own top-left corner is now at the top-right of the screen.
  assert.deepEqual(places, ["top-right", "bottom-right", "bottom-left", "top-left"]);
  const half = cornerPlaces(boxCorners({ cx: 100, cy: 100, w: 60, h: 40, angle: 180 }));
  assert.deepEqual(half, ["bottom-right", "bottom-left", "top-left", "top-right"]);
});

test("a thin box at 45 degrees still gets four different places", () => {
  for (const angle of [45, 135, -45, 30, 60]) {
    const places = cornerPlaces(boxCorners({ cx: 100, cy: 100, w: 200, h: 2, angle }));
    assert.equal(new Set(places).size, 4, `angle ${angle}: ${places.join()}`);
  }
});
