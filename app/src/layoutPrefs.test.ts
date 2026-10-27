import assert from "node:assert/strict";
import { test } from "node:test";
import { openLayout, parseLayout, serializeLayout } from "./layoutPrefs.ts";

test("nothing stored means both panels open", () => {
  assert.deepEqual(parseLayout(null), openLayout());
  assert.deepEqual(parseLayout(undefined), openLayout());
  assert.deepEqual(parseLayout(""), openLayout());
});

test("a saved layout comes back as it was", () => {
  for (const queueCollapsed of [false, true]) {
    for (const panelCollapsed of [false, true]) {
      const prefs = { queueCollapsed, panelCollapsed };
      assert.deepEqual(parseLayout(serializeLayout(prefs)), prefs);
    }
  }
});

test("garbage in storage falls back to open instead of throwing", () => {
  for (const text of ["not json", "42", "null", "[]", '"queueCollapsed"', '{"queueCollapsed":"yes","panelCollapsed":1}']) {
    assert.deepEqual(parseLayout(text), openLayout(), text);
  }
});

test("a missing field counts as open, so an older value still loads", () => {
  assert.deepEqual(parseLayout('{"queueCollapsed":true}'), { queueCollapsed: true, panelCollapsed: false });
});
