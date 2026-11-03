import assert from "node:assert/strict";
import { test } from "node:test";
import { describeDate, parseDate } from "./dates.ts";

const read = (text: string) => parseDate(text);

test("a plain year", () => {
  assert.deepEqual(read("1975"), { year: 1975, approximate: false });
  assert.deepEqual(read("  1975  "), { year: 1975, approximate: false });
});

test("a full date in numbers, year first or day first", () => {
  assert.deepEqual(read("1975-06-14"), { year: 1975, month: 6, day: 14, approximate: false });
  assert.deepEqual(read("1975-06"), { year: 1975, month: 6, approximate: false });
  assert.deepEqual(read("1975/6/14"), { year: 1975, month: 6, day: 14, approximate: false });
  assert.deepEqual(read("14.3.1982"), { year: 1982, month: 3, day: 14, approximate: false });
  assert.deepEqual(read("14/03/1982"), { year: 1982, month: 3, day: 14, approximate: false });
  // 5/6/1975 is read day first.
  assert.deepEqual(read("5/6/1975"), { year: 1975, month: 6, day: 5, approximate: false });
});

test("impossible days and months keep what is still possible", () => {
  assert.deepEqual(read("31.4.1982"), { year: 1982, month: 4, approximate: false });
  assert.deepEqual(read("1975-13-01"), { year: 1975, approximate: false });
  assert.deepEqual(read("30 February 1982"), { year: 1982, month: 2, approximate: false });
  assert.deepEqual(read("29 February 1980"), { year: 1980, month: 2, day: 29, approximate: false });
});

test("month names", () => {
  assert.deepEqual(read("14 March 1982"), { year: 1982, month: 3, day: 14, approximate: false });
  assert.deepEqual(read("March 14th, 1982"), { year: 1982, month: 3, day: 14, approximate: false });
  assert.deepEqual(read("March 1982"), { year: 1982, month: 3, approximate: false });
  assert.deepEqual(read("1982 Mar"), { year: 1982, month: 3, approximate: false });
  assert.deepEqual(read("3rd of June 1975"), { year: 1975, month: 6, day: 3, approximate: false });
  assert.deepEqual(read("Sept 1975"), { year: 1975, month: 9, approximate: false });
});

test("may is only a month next to a number", () => {
  assert.deepEqual(read("May 1975"), { year: 1975, month: 5, approximate: false });
  assert.deepEqual(read("12 May 1975"), { year: 1975, month: 5, day: 12, approximate: false });
  assert.deepEqual(read("we may have been there in 1975"), { year: 1975, approximate: false });
});

test("approximate wording", () => {
  for (const text of ["circa 1980", "ca. 1980", "c. 1980", "about 1980", "around 1980", "~1980", "1980?", "early 1980", "approx 1980"]) {
    assert.deepEqual(read(text), { year: 1980, approximate: true }, text);
  }
  assert.deepEqual(read("1980s"), { year: 1980, approximate: true });
  assert.deepEqual(read("circa June 1980"), { year: 1980, month: 6, approximate: true });
});

test("seasons give the year only", () => {
  assert.deepEqual(read("summer 1975"), { year: 1975, approximate: false });
  assert.deepEqual(read("Winter '75 or 1976"), { year: 1976, approximate: false });
  assert.deepEqual(read("autumn 1975"), { year: 1975, approximate: false });
});

test("no usable year", () => {
  assert.equal(read(""), null);
  assert.equal(read("   "), null);
  assert.equal(read("a long time ago"), null);
  assert.equal(read("1492"), null);
  assert.equal(read("12345"), null);
  assert.equal(read(String(new Date().getFullYear() + 5)), null);
  // Next year is accepted, in case a date is a little ahead.
  assert.equal(read(String(new Date().getFullYear() + 1))?.year, new Date().getFullYear() + 1);
});

test("what a person is told", () => {
  assert.equal(describeDate(""), "");
  assert.equal(describeDate("1975"), "Saved as 1975.");
  assert.equal(describeDate("circa 1980"), "Saved as about 1980.");
  assert.equal(describeDate("14 March 1982"), "Saved as 14 March 1982.");
  assert.equal(describeDate("June 1975"), "Saved as June 1975.");
  assert.equal(describeDate("long ago"), "No year found, so it is kept as you wrote it.");
});
