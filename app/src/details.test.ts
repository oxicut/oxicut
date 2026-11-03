import assert from "node:assert/strict";
import { test } from "node:test";
import {
  MAX_CAPTION,
  MAX_PEOPLE,
  MAX_TEXT,
  detailsLine,
  fillEmpty,
  isEmptyDetails,
  limitDetails,
  mergeDetails,
  noDetails,
  parsePeople,
  peopleText,
  toExportDetails,
} from "./details.ts";

const scan = { date: "summer 1975", place: "Balaton", caption: "Beach day", people: ["Anna", "Péter"] };

test("nothing typed is empty", () => {
  assert.equal(isEmptyDetails(noDetails()), true);
  assert.equal(isEmptyDetails({ place: "   ", people: [" "] }), true);
  assert.equal(isEmptyDetails(undefined), true);
  assert.equal(isEmptyDetails({ caption: "x" }), false);
});

test("people are split on commas, semicolons and new lines, once each", () => {
  assert.deepEqual(parsePeople("Anna, Péter; Grandma\nanna ,, "), ["Anna", "Péter", "Grandma"]);
  assert.deepEqual(parsePeople(""), []);
  assert.equal(peopleText(["Anna", "Péter"]), "Anna, Péter");
  assert.equal(parsePeople(Array.from({ length: 150 }, (_, i) => `P${i}`).join(",")).length, MAX_PEOPLE);
  assert.equal(parsePeople("x".repeat(500))[0]?.length, MAX_TEXT);
});

test("limits cut texts and tidy people", () => {
  const d = limitDetails({ date: ` ${"d".repeat(300)} `, place: " Balaton ", caption: "é".repeat(MAX_CAPTION + 10), people: ["Anna", "ANNA", " "] });
  assert.equal(d.date.length, MAX_TEXT);
  assert.equal(d.place, "Balaton");
  assert.equal(d.caption.length, MAX_CAPTION);
  assert.deepEqual(d.people, ["Anna"]);
});

test("a picture's own details win field by field, and its people replace the list", () => {
  assert.deepEqual(mergeDetails(scan, undefined), scan);
  assert.deepEqual(mergeDetails(scan, noDetails()), scan);
  const merged = mergeDetails(scan, { caption: "Just this one", people: ["Grandma"] });
  assert.deepEqual(merged, { date: "summer 1975", place: "Balaton", caption: "Just this one", people: ["Grandma"] });
  // An empty field in the picture's own details means "use the scan's".
  assert.equal(mergeDetails(scan, { place: "  " }).place, "Balaton");
});

test("copying to other scans only fills what is empty", () => {
  const other = { date: "", place: "Siófok", caption: "", people: [] };
  assert.deepEqual(fillEmpty(scan, other), { date: "summer 1975", place: "Siófok", caption: "Beach day", people: ["Anna", "Péter"] });
  assert.deepEqual(fillEmpty(scan, scan), scan);
});

test("what the export is sent", () => {
  assert.equal(toExportDetails(noDetails()), null);
  const sent = toExportDetails({ ...scan, date: "14 June 1975" });
  assert.deepEqual(sent, {
    dateText: "14 June 1975",
    year: 1975,
    month: 6,
    day: 14,
    approximate: false,
    place: "Balaton",
    caption: "Beach day",
    people: ["Anna", "Péter"],
  });
  // A date with no year is still kept, as wording only.
  const wording = toExportDetails({ ...noDetails(), date: "long ago" });
  assert.deepEqual(wording, { dateText: "long ago", approximate: false, place: "", caption: "", people: [] });
  assert.equal(toExportDetails({ ...noDetails(), date: "circa 1980" })?.approximate, true);
});

test("the short line on a card", () => {
  assert.equal(detailsLine(scan), "summer 1975 · Balaton");
  assert.equal(detailsLine({ ...scan, date: "" }), "Balaton");
  assert.equal(detailsLine(noDetails()), "");
  assert.equal(detailsLine({ ...noDetails(), place: "A very long place name indeed" }), "A very long place name …");
});
