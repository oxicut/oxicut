// Turning the scan list into what a `.oxicut` file holds and back. No Vue here, so it is easy to test.

import type { Box, ProjectFile, ScanRecord } from "./api";
import type { Pair } from "./pairing.ts";

/** The part of a scan that is kept in a project (previews, thumbnails, history and flags are rebuilt). */
export interface SavedScan {
  id: number;
  path: string;
  name: string;
  width: number;
  height: number;
  status: string;
  /** Boxes as they are on screen (with their ids) or as they were saved (without). */
  boxes: (Box & { id?: number })[];
  detectedKey: string;
  exportedKey: string | null;
  exportedFiles: string[];
  /** The scan holding the backs of this scan's pictures, when this is a scan of fronts. */
  backsOn: number | null;
  /** Which picture on this scan goes with which on `backsOn` (box ids). */
  pairs: Pair[];
  error: string | null;
}

/** Statuses a scan can come back with. A scan that was still waiting or running when the app closed is detected again. */
export type RestoredStatus = "queued" | "pending" | "flagged" | "approved" | "error";

export function restoreStatus(status: unknown): RestoredStatus {
  return status === "pending" || status === "flagged" || status === "approved" || status === "error" ? status : "queued";
}

/** The pairs of `scan` as positions in the two box lists (box ids are made up fresh on open). */
export function pairPositions(scan: SavedScan, all: SavedScan[]): [number, number][] {
  const backs = scan.backsOn === null ? undefined : all.find((s) => s.id === scan.backsOn);
  if (!backs) return [];
  const positions: [number, number][] = [];
  for (const pair of scan.pairs) {
    const front = scan.boxes.findIndex((b) => b.id === pair.front);
    const back = backs.boxes.findIndex((b) => b.id === pair.back);
    if (front >= 0 && back >= 0) positions.push([front, back]);
  }
  return positions;
}

/** The pairs for saved positions, once the two scans' boxes have their ids. */
export function pairsFromPositions(positions: [number, number][], frontBoxes: { id: number }[], backBoxes: { id: number }[]): Pair[] {
  const pairs: Pair[] = [];
  for (const [front, back] of positions) {
    const f = frontBoxes[front];
    const b = backBoxes[back];
    if (f && b) pairs.push({ front: f.id, back: b.id });
  }
  return pairs;
}

/** `all` is every scan of the project: a pair is saved as positions, which needs the other scan's boxes. */
export function toRecord(scan: SavedScan, all: SavedScan[] = [scan]): ScanRecord {
  return {
    id: scan.id,
    path: scan.path,
    name: scan.name,
    width: scan.width,
    height: scan.height,
    status: scan.status,
    boxes: scan.boxes.map(({ cx, cy, w, h, angle }) => ({ cx, cy, w, h, angle })),
    detectedKey: scan.detectedKey,
    exportedKey: scan.exportedKey,
    exportedFiles: [...scan.exportedFiles],
    backsOn: scan.backsOn,
    pairs: pairPositions(scan, all),
    error: scan.error,
  };
}

/** A record as the app wants it, whatever an older or newer file left out. */
export function fromRecord(
  record: Partial<ScanRecord> & { id: number; path: string },
): SavedScan & { status: RestoredStatus; pairPositions: [number, number][] } {
  const name = record.name || record.path.split(/[\\/]/).pop() || record.path;
  const boxes = Array.isArray(record.boxes) ? record.boxes : [];
  // A scan never measured (size 0) was not analysed yet, whatever status it claims.
  const measured = (record.width ?? 0) > 0 && (record.height ?? 0) > 0;
  return {
    id: record.id,
    path: record.path,
    name,
    width: record.width ?? 0,
    height: record.height ?? 0,
    status: measured ? restoreStatus(record.status) : "queued",
    boxes: boxes.map((b) => ({ cx: b.cx, cy: b.cy, w: b.w, h: b.h, angle: b.angle })),
    detectedKey: record.detectedKey ?? "",
    exportedKey: record.exportedKey ?? null,
    exportedFiles: Array.isArray(record.exportedFiles) ? record.exportedFiles.filter((f) => typeof f === "string") : [],
    backsOn: typeof record.backsOn === "number" ? record.backsOn : null,
    pairs: [],
    // Positions in the box lists; the store turns them into box ids once every scan has its boxes.
    pairPositions: Array.isArray(record.pairs)
      ? record.pairs.filter((p): p is [number, number] => Array.isArray(p) && Number.isInteger(p[0]) && Number.isInteger(p[1]))
      : [],
    error: record.error ?? null,
  };
}

export function toProjectFile(scans: SavedScan[], created: number): ProjectFile {
  return { formatVersion: 1, appVersion: "", created, updated: 0, scans: scans.map((s) => toRecord(s, scans)) };
}

/** Changes whenever something worth saving does; the app saves when this differs from the last save. */
export function projectKey(scans: SavedScan[]): string {
  return JSON.stringify(scans.map((s) => toRecord(s, scans)));
}

/** The next free scan id after the ones a project already uses. */
export function nextIdAfter(records: { id: number }[]): number {
  return records.reduce((max, r) => Math.max(max, r.id), 0) + 1;
}
