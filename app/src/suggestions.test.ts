import assert from "node:assert/strict";
import { test } from "node:test";
import { describeSuggestion } from "./suggestions.ts";

const box = { cx: 500, cy: 350, w: 500, h: 400, angle: 0 };

test("a smaller box cuts closer and a larger one cuts wider", () => {
  assert.equal(describeSuggestion(box, { ...box, w: 468, h: 368 }), "Cuts closer");
  assert.equal(describeSuggestion(box, { ...box, w: 540, h: 440 }), "Cuts wider");
});

test("a box nearer level is straighter, and any other tilt change is said so", () => {
  const tilted = { ...box, angle: 3 };
  assert.equal(describeSuggestion(tilted, { ...tilted, angle: 0.4 }), "Straighter");
  assert.equal(describeSuggestion(box, { ...box, angle: 2 }), "Tilted differently");
  // A card turned a quarter is level too.
  assert.equal(describeSuggestion({ ...box, angle: -87 }, { ...box, angle: -90 }), "Straighter");
});

test("several changes are put together in plain words", () => {
  const tilted = { ...box, angle: 4 };
  assert.equal(describeSuggestion(tilted, { ...tilted, w: 468, h: 368, angle: 0 }), "Cuts closer and straighter");
});

test("a change in proportions is a different shape", () => {
  assert.equal(describeSuggestion(box, { ...box, w: 500, h: 330 }), "Cuts closer and a different shape");
  assert.equal(describeSuggestion(box, { ...box, w: 560, h: 400 }), "Cuts wider and a different shape");
});

test("only a shift is moved a little, and no visible change is still a different cut", () => {
  assert.equal(describeSuggestion(box, { ...box, cx: 520 }), "Moved a little");
  assert.equal(describeSuggestion(box, { ...box, cx: 501 }), "A different cut");
});
