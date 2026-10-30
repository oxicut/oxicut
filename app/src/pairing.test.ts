import assert from "node:assert/strict";
import { test } from "node:test";
import { activePairs, matchBacks, planCrops, partnerScan, withPair, withoutPair, type PairScan } from "./pairing.ts";

function scan(id: number, name: string, boxes: [number, number, number][], extra: Partial<PairScan> = {}): PairScan {
  return {
    id,
    name,
    width: 1000,
    height: 1000,
    boxes: boxes.map(([boxId, cx, cy]) => ({ id: boxId, cx, cy })),
    backsOn: null,
    pairs: [],
    ...extra,
  };
}

// Four cards on the glass: top left, top right, bottom left, bottom right.
const fronts = scan(1, "cards.jpg", [[1, 250, 250], [2, 750, 250], [3, 250, 750], [4, 750, 750]]);

test("a stack turned over like a book page is matched mirrored left to right", () => {
  const backs = scan(2, "cards-backs.jpg", [[11, 750, 250], [12, 250, 250], [13, 750, 750], [14, 250, 750]]);
  assert.deepEqual(matchBacks(fronts, backs), [
    { front: 1, back: 11 },
    { front: 2, back: 12 },
    { front: 3, back: 13 },
    { front: 4, back: 14 },
  ]);
});

test("a stack turned over towards you is matched mirrored top to bottom", () => {
  // Three cards in an L: the layout is not the same mirrored both ways, so it tells the two apart.
  const three = scan(1, "cards.jpg", [[1, 250, 250], [2, 750, 250], [3, 250, 750]]);
  const backs = scan(2, "cards-backs.jpg", [[11, 250, 750], [12, 750, 750], [13, 250, 250]]);
  assert.deepEqual(matchBacks(three, backs), [
    { front: 1, back: 11 },
    { front: 2, back: 12 },
    { front: 3, back: 13 },
  ]);
});

test("when both ways fit equally well, left to right wins", () => {
  const backs = scan(2, "cards-backs.jpg", [[11, 250, 750], [12, 750, 750], [13, 250, 250], [14, 750, 250]]);
  assert.deepEqual(matchBacks(fronts, backs).map((p) => p.back), [14, 13, 12, 11]);
});

test("tilted and shifted cards still find their back", () => {
  const backs = scan(2, "b.jpg", [[11, 790, 230], [12, 240, 270], [13, 720, 770], [14, 270, 730]]);
  assert.deepEqual(matchBacks(fronts, backs).map((p) => p.back), [11, 12, 13, 14]);
});

test("more fronts than backs leaves the extra fronts unpaired, and the other way round", () => {
  const two = scan(2, "b.jpg", [[11, 750, 250], [12, 250, 250]]);
  assert.deepEqual(matchBacks(fronts, two), [{ front: 1, back: 11 }, { front: 2, back: 12 }]);
  const six = scan(3, "c.jpg", [[21, 750, 250], [22, 250, 250], [23, 750, 750], [24, 250, 750], [25, 500, 500], [26, 100, 900]]);
  const pairs = matchBacks(fronts, six);
  assert.equal(pairs.length, 4);
  assert.deepEqual(new Set(pairs.map((p) => p.front)).size, 4);
});

test("a scan without pictures matches nothing", () => {
  assert.deepEqual(matchBacks(fronts, scan(2, "b.jpg", [])), []);
  assert.deepEqual(matchBacks(scan(2, "b.jpg", []), fronts), []);
});

test("pairs whose picture was removed are not active, and come back when the picture does", () => {
  const pairs = [{ front: 1, back: 11 }, { front: 2, back: 12 }];
  assert.deepEqual(activePairs(pairs, [{ id: 1 }], [{ id: 11 }, { id: 12 }]), [{ front: 1, back: 11 }]);
  assert.deepEqual(activePairs(pairs, [{ id: 1 }, { id: 2 }], [{ id: 11 }, { id: 12 }]), pairs);
});

test("choosing another back takes it from its old front", () => {
  const pairs = [{ front: 1, back: 11 }, { front: 2, back: 12 }];
  assert.deepEqual(withPair(pairs, 1, 12), [{ front: 1, back: 12 }]);
  assert.deepEqual(withoutPair(pairs, 2), [{ front: 1, back: 11 }]);
});

const naming = { pattern: "{source}-{n}", format: "same" as const };

test("pairs are called front and back after the front's usual name", () => {
  const linked = { ...fronts, backsOn: 2, pairs: [{ front: 2, back: 12 }] };
  const backs = scan(2, "cards-backs.jpg", [[11, 100, 100], [12, 200, 200]]);
  const all = [linked, backs];
  const front = planCrops(all, linked, naming);
  assert.equal(front[1]?.fileName, "cards-2-front.jpg");
  assert.equal(front[1]?.chosenName, "cards-2-front");
  assert.equal(front[1]?.role, "front");
  assert.deepEqual(front[1]?.partner, { scanId: 2, boxId: 12 });
  // A front without a back keeps its usual name.
  assert.equal(front[0]?.fileName, "cards-1.jpg");
  assert.equal(front[0]?.chosenName, null);

  const back = planCrops(all, backs, naming);
  assert.equal(back[1]?.fileName, "cards-2-back.jpg");
  assert.equal(back[1]?.role, "back");
  assert.equal(back[0]?.fileName, "cards-backs-1.jpg");
});

test("a pair keeps the file type of the scan each picture comes from", () => {
  const linked = { ...fronts, name: "cards.tif", backsOn: 2, pairs: [{ front: 1, back: 11 }] };
  const backs = scan(2, "backs.jpg", [[11, 100, 100]]);
  assert.equal(planCrops([linked, backs], linked, naming)[0]?.fileName, "cards-1-front.tif");
  assert.equal(planCrops([linked, backs], backs, naming)[0]?.fileName, "cards-1-back.jpg");
});

test("a pair whose picture is gone falls back to the usual names", () => {
  const linked = { ...fronts, backsOn: 2, pairs: [{ front: 1, back: 99 }] };
  const backs = scan(2, "b.jpg", [[11, 100, 100]]);
  assert.equal(planCrops([linked, backs], linked, naming)[0]?.fileName, "cards-1.jpg");
});

test("the partner scan is found from either side", () => {
  const linked = { ...fronts, backsOn: 2 };
  const backs = scan(2, "b.jpg", []);
  assert.equal(partnerScan([linked, backs], linked)?.id, 2);
  assert.equal(partnerScan([linked, backs], backs)?.id, 1);
  assert.equal(partnerScan([scan(5, "x.jpg", [])], scan(5, "x.jpg", [])), undefined);
});
