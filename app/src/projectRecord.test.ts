import assert from "node:assert/strict";
import { test } from "node:test";
import { fromRecord, nextIdAfter, projectKey, restoreStatus, toProjectFile, toRecord } from "./projectRecord.ts";

const scan = {
  id: 4,
  path: "D:/scans/a.tif",
  name: "a.tif",
  width: 1000,
  height: 700,
  status: "approved",
  boxes: [{ cx: 10, cy: 20, w: 30, h: 40, angle: 5, id: 99 } as never],
  detectedKey: "k",
  exportedKey: null,
  error: null,
};

test("a record keeps the boxes without their screen ids", () => {
  const record = toRecord(scan);
  assert.deepEqual(record.boxes, [{ cx: 10, cy: 20, w: 30, h: 40, angle: 5 }]);
  assert.equal("id" in record.boxes[0], false);
});

test("a scan comes back from its record as it was", () => {
  const back = fromRecord(toRecord(scan));
  assert.equal(back.status, "approved");
  assert.deepEqual(back.boxes, [{ cx: 10, cy: 20, w: 30, h: 40, angle: 5 }]);
  assert.equal(back.detectedKey, "k");
});

test("scans that were still waiting or running are detected again", () => {
  for (const status of ["queued", "detecting", "something new", undefined, 3]) {
    assert.equal(restoreStatus(status), "queued", String(status));
  }
  for (const status of ["pending", "flagged", "approved", "error"] as const) assert.equal(restoreStatus(status), status);
  // A scan that never got analysed has no size, so it cannot be approved or flagged.
  assert.equal(fromRecord({ id: 1, path: "x.png", status: "approved" }).status, "queued");
});

test("an old or hand-edited record with fields missing still loads", () => {
  const back = fromRecord({ id: 2, path: "D:\\s\\b.jpg" });
  assert.equal(back.name, "b.jpg");
  assert.deepEqual(back.boxes, []);
  assert.equal(back.exportedKey, null);
  assert.equal(back.error, null);
});

test("the save key changes with the boxes and the status but not with where in memory they live", () => {
  const same = { ...scan, boxes: scan.boxes.map((b) => ({ ...b, id: 123 })) };
  assert.equal(projectKey([scan]), projectKey([same]));
  assert.notEqual(projectKey([scan]), projectKey([{ ...scan, status: "pending" }]));
  assert.notEqual(projectKey([scan]), projectKey([{ ...scan, boxes: [{ ...scan.boxes[0], angle: 6 }] }]));
  assert.notEqual(projectKey([scan]), projectKey([{ ...scan, exportedKey: "k" }]));
});

test("ids continue after the largest one used, and a project file lists every scan", () => {
  assert.equal(nextIdAfter([]), 1);
  assert.equal(nextIdAfter([{ id: 3 }, { id: 9 }, { id: 5 }]), 10);
  assert.equal(toProjectFile([scan, { ...scan, id: 5 }], 77).scans.length, 2);
  assert.equal(toProjectFile([], 77).created, 77);
});
