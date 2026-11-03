import assert from "node:assert/strict";
import { test } from "node:test";
import { blocksOf, isFollowable, spansOf } from "./changelogView.ts";

test("bold, code and project links are understood", () => {
  assert.deepEqual(spansOf("A **new** `button` in [the guide](https://github.com/oxicut/oxicut/issues/1)."), [
    { kind: "text", text: "A " },
    { kind: "bold", text: "new" },
    { kind: "text", text: " " },
    { kind: "code", text: "button" },
    { kind: "text", text: " in " },
    { kind: "link", text: "the guide", url: "https://github.com/oxicut/oxicut/issues/1" },
    { kind: "text", text: "." },
  ]);
});

test("only the project's own pages are followed; other links show as plain text", () => {
  assert.equal(isFollowable("https://github.com/oxicut/oxicut"), true);
  assert.equal(isFollowable("https://oxicut.github.io/guide"), true);
  assert.equal(isFollowable("https://github.com/evil/oxicut"), false);
  assert.equal(isFollowable("http://github.com/oxicut/x"), false);
  assert.equal(isFollowable("javascript:alert(1)"), false);
  assert.deepEqual(spansOf("[click](https://example.com/x)"), [{ kind: "text", text: "click" }]);
  assert.equal(spansOf("[click](javascript:alert(1))").some((span) => span.kind === "link"), false);
});

test("html in the text stays text", () => {
  assert.deepEqual(spansOf("<img src=x onerror=alert(1)>"), [{ kind: "text", text: "<img src=x onerror=alert(1)>" }]);
  assert.deepEqual(blocksOf("<b>hi</b>"), [{ kind: "paragraph", spans: [{ kind: "text", text: "<b>hi</b>" }] }]);
});

test("headings, bullet lists and paragraphs become blocks", () => {
  const blocks = blocksOf(`### Added
- First **thing**
- Second thing that runs
  onto a second line

### Fixed
Plain words.
More words.
`);
  assert.deepEqual(blocks, [
    { kind: "heading", spans: [{ kind: "text", text: "Added" }] },
    {
      kind: "list",
      items: [
        [{ kind: "text", text: "First " }, { kind: "bold", text: "thing" }],
        [{ kind: "text", text: "Second thing that runs onto a second line" }],
      ],
    },
    { kind: "heading", spans: [{ kind: "text", text: "Fixed" }] },
    { kind: "paragraph", spans: [{ kind: "text", text: "Plain words. More words." }] },
  ]);
  assert.deepEqual(blocksOf(""), []);
});
