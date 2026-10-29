// Turning the scan list into what a `.oxicut` file holds and back. No Vue here, so it is easy to test.

import type { Box, ProjectFile, ScanRecord } from "./api";

/** The part of a scan that is kept in a project (previews, thumbnails, history and flags are rebuilt). */
export interface SavedScan {
  id: number;
  path: string;
  name: string;
  width: number;
  height: number;
  status: string;
  boxes: Box[];
  detectedKey: string;
  exportedKey: string | null;
  exportedFiles: string[];
  error: string | null;
}

/** Statuses a scan can come back with. A scan that was still waiting or running when the app closed is detected again. */
export type RestoredStatus = "queued" | "pending" | "flagged" | "approved" | "error";

export function restoreStatus(status: unknown): RestoredStatus {
  return status === "pending" || status === "flagged" || status === "approved" || status === "error" ? status : "queued";
}

export function toRecord(scan: SavedScan): ScanRecord {
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
    error: scan.error,
  };
}

/** A record as the app wants it, whatever an older or newer file left out. */
export function fromRecord(record: Partial<ScanRecord> & { id: number; path: string }): SavedScan & { status: RestoredStatus } {
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
    error: record.error ?? null,
  };
}

export function toProjectFile(scans: SavedScan[], created: number): ProjectFile {
  return { formatVersion: 1, appVersion: "", created, updated: 0, scans: scans.map(toRecord) };
}

/** Changes whenever something worth saving does; the app saves when this differs from the last save. */
export function projectKey(scans: SavedScan[]): string {
  return JSON.stringify(scans.map(toRecord));
}

/** The next free scan id after the ones a project already uses. */
export function nextIdAfter(records: { id: number }[]): number {
  return records.reduce((max, r) => Math.max(max, r.id), 0) + 1;
}
