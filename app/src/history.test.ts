import assert from "node:assert/strict";
import { test } from "node:test";
import { History, sameBoxes } from "./history.ts";

const box = (id: number, cx = 10) => ({ id, cx, cy: 20, w: 100, h: 80, angle: 0 });

test("undo and redo walk back and forth through the recorded states", () => {
  const history = new History<string>();
  history.record("a"); // a -> b
  history.record("b"); // b -> c
  assert.equal(history.undo("c"), "b");
  assert.equal(history.undo("b"), "a");
  assert.equal(history.undo("a"), undefined);
  assert.equal(history.redo("a"), "b");
  assert.equal(history.redo("b"), "c");
  assert.equal(history.redo("c"), undefined);
  assert.ok(history.canUndo && !history.canRedo);
});

test("a new edit ends what could have been redone", () => {
  const history = new History<string>();
  history.record("a");
  assert.equal(history.undo("b"), "a");
  assert.ok(history.canRedo);
  history.record("a");
  assert.ok(!history.canRedo);
});

test("the oldest states are dropped past the limit", () => {
  const history = new History<number>(3);
  for (let i = 0; i < 5; i++) history.record(i);
  assert.deepEqual(history.past, [2, 3, 4]);
});

test("clear forgets everything", () => {
  const history = new History<number>();
  history.record(1);
  history.undo(2);
  history.clear();
  assert.ok(!history.canUndo && !history.canRedo);
});

test("sameBoxes ignores sub-pixel noise but sees real changes", () => {
  assert.ok(sameBoxes([box(1)], [box(1, 10.005)]));
  assert.ok(!sameBoxes([box(1)], [box(1, 11)]));
  assert.ok(!sameBoxes([box(1)], [box(2)]));
  assert.ok(!sameBoxes([box(1)], [box(1), box(2)]));
  assert.ok(sameBoxes([], []));
});
