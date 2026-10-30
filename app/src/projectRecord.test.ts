import assert from "node:assert/strict";
import { test } from "node:test";
import { fromRecord, nextIdAfter, pairsFromPositions, projectKey, restoreStatus, toProjectFile, toRecord } from "./projectRecord.ts";

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
  exportedFiles: ["D:/out/a-1.png"],
  backsOn: null,
  pairs: [],
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
  assert.deepEqual(back.exportedFiles, ["D:/out/a-1.png"]);
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
  assert.deepEqual(back.exportedFiles, []);
  assert.deepEqual(fromRecord({ id: 3, path: "x.png", exportedFiles: ["ok.png", 7 as never] }).exportedFiles, ["ok.png"]);
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

test("pairs are saved as positions and come back as positions", () => {
  const box = (id: number) => ({ cx: id, cy: id, w: 1, h: 1, angle: 0, id });
  const fronts = { ...scan, id: 1, boxes: [box(10), box(11), box(12)], backsOn: 2, pairs: [{ front: 11, back: 21 }, { front: 12, back: 20 }, { front: 10, back: 99 }] };
  const backs = { ...scan, id: 2, boxes: [box(20), box(21)] };
  const record = toRecord(fronts, [fronts, backs]);
  // A pair whose picture does not exist any more is not saved.
  assert.deepEqual(record.pairs, [[1, 1], [2, 0]]);
  assert.equal(record.backsOn, 2);
  const back = fromRecord(record);
  assert.equal(back.backsOn, 2);
  assert.deepEqual(back.pairPositions, [[1, 1], [2, 0]]);
  assert.deepEqual(pairsFromPositions(back.pairPositions, [box(30), box(31), box(32)], [box(40), box(41)]), [
    { front: 31, back: 41 },
    { front: 32, back: 40 },
  ]);
  // Positions that point at nothing are dropped.
  assert.deepEqual(pairsFromPositions([[5, 0], [0, 7]], [box(30)], [box(40)]), []);
});

test("an old record without pairs opens unpaired, and a pairing change changes what is saved", () => {
  const old = fromRecord({ id: 1, path: "x.png" });
  assert.equal(old.backsOn, null);
  assert.deepEqual(old.pairPositions, []);
  const before = projectKey([scan]);
  assert.notEqual(before, projectKey([{ ...scan, backsOn: 9 }, { ...scan, id: 9 }]));
});
