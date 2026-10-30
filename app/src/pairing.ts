// Fronts and backs: a scan of the fronts of a stack of prints, and a scan of the same stack turned over.
// Pure functions (no Vue), so the rules are tested without a window.

import { cropFileName, expandPattern, outputExtension, type OutputFormat } from "./cropNames.ts";

/** One picture on a scan, as far as pairing cares. */
export interface PairBox {
  id: number;
  cx: number;
  cy: number;
}

/** Which picture on a scan of fronts goes with which on the scan of backs (box ids). */
export interface Pair {
  front: number;
  back: number;
}

/** The part of a scan that pairing looks at. */
export interface PairScan {
  id: number;
  name: string;
  width: number;
  height: number;
  boxes: PairBox[];
  /** The scan holding the backs of this scan's pictures; set on the scan of fronts only. */
  backsOn: number | null;
  pairs: Pair[];
}

type Flip = "left-right" | "top-bottom";

/** Where a picture sits on its scan, 0 to 1 across and down, mirrored when the stack was turned over. */
function place(box: PairBox, scan: { width: number; height: number }, flip: Flip | null): [number, number] {
  const x = scan.width > 0 ? box.cx / scan.width : 0;
  const y = scan.height > 0 ? box.cy / scan.height : 0;
  return [flip === "left-right" ? 1 - x : x, flip === "top-bottom" ? 1 - y : y];
}

/** Pairs by nearest first; returns the pairs and how far apart they are in total. */
function assign(frontScan: PairScan, backScan: PairScan, flip: Flip): { pairs: Pair[]; cost: number } {
  const candidates: { front: number; back: number; distance: number }[] = [];
  for (const f of frontScan.boxes) {
    const [fx, fy] = place(f, frontScan, null);
    for (const b of backScan.boxes) {
      const [bx, by] = place(b, backScan, flip);
      candidates.push({ front: f.id, back: b.id, distance: Math.hypot(fx - bx, fy - by) });
    }
  }
  candidates.sort((a, b) => a.distance - b.distance || a.front - b.front || a.back - b.back);
  const usedFronts = new Set<number>();
  const usedBacks = new Set<number>();
  const pairs: Pair[] = [];
  let cost = 0;
  for (const c of candidates) {
    if (usedFronts.has(c.front) || usedBacks.has(c.back)) continue;
    usedFronts.add(c.front);
    usedBacks.add(c.back);
    pairs.push({ front: c.front, back: c.back });
    cost += c.distance;
  }
  return { pairs, cost };
}

/**
 * Match the pictures on a scan of fronts with the pictures on the scan of backs. A stack turned over like
 * the page of a book lands mirrored left to right; turned over towards you, top to bottom. Both are tried
 * and the one that fits better wins (left to right on a tie). Pictures without a partner stay unpaired.
 */
export function matchBacks(frontScan: PairScan, backScan: PairScan): Pair[] {
  if (frontScan.boxes.length === 0 || backScan.boxes.length === 0) return [];
  const sideways = assign(frontScan, backScan, "left-right");
  const upright = assign(frontScan, backScan, "top-bottom");
  const best = upright.cost < sideways.cost ? upright : sideways;
  // In the order of the fronts, which is the order of the file names.
  const order = new Map(frontScan.boxes.map((b, i) => [b.id, i]));
  return [...best.pairs].sort((a, b) => (order.get(a.front) ?? 0) - (order.get(b.front) ?? 0));
}

/** The pairs whose two pictures both still exist (a removed picture can come back with Undo). */
export function activePairs(pairs: Pair[], frontBoxes: { id: number }[], backBoxes: { id: number }[]): Pair[] {
  const fronts = new Set(frontBoxes.map((b) => b.id));
  const backs = new Set(backBoxes.map((b) => b.id));
  return pairs.filter((p) => fronts.has(p.front) && backs.has(p.back));
}

/** Put `front` and `back` together, taking either out of any pair it was in. */
export function withPair(pairs: Pair[], front: number, back: number): Pair[] {
  return [...pairs.filter((p) => p.front !== front && p.back !== back), { front, back }];
}

/** The pairs without the one holding this front picture. */
export function withoutPair(pairs: Pair[], front: number): Pair[] {
  return pairs.filter((p) => p.front !== front);
}

/** The scan of fronts whose backs are on `scan`, if there is one. */
export function frontScanOf(scans: PairScan[], scan: PairScan): PairScan | undefined {
  return scans.find((s) => s.backsOn === scan.id);
}

/** The other scan of a linked pair of scans: the backs for a scan of fronts, the fronts for a scan of backs. */
export function partnerScan(scans: PairScan[], scan: PairScan): PairScan | undefined {
  if (scan.backsOn !== null) return scans.find((s) => s.id === scan.backsOn);
  return frontScanOf(scans, scan);
}

/** How a picture is taking part in a pair. */
export type Role = "front" | "back";

/** What an export will call a picture on a scan, and which partner it has. */
export interface CropPlan {
  /** The file name with its extension. */
  fileName: string;
  /** The name without the extension, as handed to the export (`null`: let the naming pattern decide). */
  chosenName: string | null;
  role: Role | null;
  /** The partner picture, when there is one. */
  partner: { scanId: number; boxId: number } | null;
}

export interface NamingSettings {
  pattern: string;
  format: OutputFormat;
}

/** The name a pair goes by: what the front would be called without pairing. */
function pairBase(frontScan: PairScan, frontIndex: number, naming: NamingSettings): string {
  return expandPattern(naming.pattern, frontScan.name.replace(/\.[^.]+$/, ""), frontIndex + 1);
}

/** Plan the file names of every picture on `scan`; `scans` is the whole list, to find its front or back. */
export function planCrops(scans: PairScan[], scan: PairScan, naming: NamingSettings): CropPlan[] {
  const backScan = scan.backsOn !== null ? scans.find((s) => s.id === scan.backsOn) : undefined;
  const frontScan = frontScanOf(scans, scan);
  const ending = (name: string) => `${name}.${outputExtension(naming.format, scan.name)}`;
  return scan.boxes.map((box, index) => {
    if (backScan) {
      const pair = activePairs(scan.pairs, scan.boxes, backScan.boxes).find((p) => p.front === box.id);
      if (pair) {
        const chosenName = `${pairBase(scan, index, naming)}-front`;
        return { fileName: ending(chosenName), chosenName, role: "front", partner: { scanId: backScan.id, boxId: pair.back } };
      }
    }
    if (frontScan) {
      const pair = activePairs(frontScan.pairs, frontScan.boxes, scan.boxes).find((p) => p.back === box.id);
      const frontIndex = pair ? frontScan.boxes.findIndex((b) => b.id === pair.front) : -1;
      if (pair && frontIndex >= 0) {
        const chosenName = `${pairBase(frontScan, frontIndex, naming)}-back`;
        return { fileName: ending(chosenName), chosenName, role: "back", partner: { scanId: frontScan.id, boxId: pair.front } };
      }
    }
    return { fileName: cropFileName(naming.pattern, scan.name, index + 1, naming.format), chosenName: null, role: null, partner: null };
  });
}
