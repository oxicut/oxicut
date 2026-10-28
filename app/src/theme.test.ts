import assert from "node:assert/strict";
import { test } from "node:test";
import { THEME_CHOICES, applyThemeToRoot, parseTheme } from "./theme.ts";

test("only light and dark are kept; anything else follows the system", () => {
  assert.equal(parseTheme("light"), "light");
  assert.equal(parseTheme("dark"), "dark");
  assert.equal(parseTheme("system"), "system");
  for (const odd of [undefined, null, "", "Dark", "auto", 3, {}, ["dark"]]) {
    assert.equal(parseTheme(odd), "system", String(odd));
  }
});

test("light and dark set the attribute and following the system removes it", () => {
  const root: { dataset: Record<string, string | undefined> } = { dataset: {} };
  applyThemeToRoot(root, "dark");
  assert.equal(root.dataset.theme, "dark");
  applyThemeToRoot(root, "light");
  assert.equal(root.dataset.theme, "light");
  applyThemeToRoot(root, "system");
  assert.equal("theme" in root.dataset, false);
});

test("the picker offers all three choices, following the system first", () => {
  assert.deepEqual(
    THEME_CHOICES.map((c) => c.id),
    ["system", "light", "dark"],
  );
  for (const choice of THEME_CHOICES) assert.ok(choice.label && choice.hint);
});
