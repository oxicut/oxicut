import assert from "node:assert/strict";
import { test } from "node:test";
import { METHODS, methodFor, methodName, methodShort } from "./detectionMethods.ts";

test("every detection method has a plain name, with the technical one in brackets", () => {
  assert.equal(methodName("v3"), "Basic (v3)");
  assert.equal(methodName("v7-lite"), "Standard (v7-lite)");
  assert.equal(methodName("v7"), "Careful (v7)");
  assert.equal(methodShort("v7-lite"), "Standard");
});

test("something unknown shows as it is rather than as nothing", () => {
  assert.equal(methodName("v8"), "v8");
  assert.equal(methodShort("v8"), "v8");
  assert.equal(methodFor("v8"), undefined);
});

test("the choices are listed most accurate first, and the notes use the plain names", () => {
  assert.deepEqual(METHODS.map((f) => f.mode), ["v7", "v7-lite", "v3"]);
  for (const f of METHODS) {
    assert.ok(f.about.startsWith(`The ${f.name} (${f.mode}) detection method`), f.about);
    for (const jargon of ["MobileSAM", "ONNX", "checksum"]) assert.ok(!f.about.includes(jargon), `${f.name}: ${jargon}`);
  }
});
