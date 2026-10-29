// What closing the window now would leave undone. Projects save themselves, so only a running export
// still needs a warning; the counts are kept for the statistics a later phase adds. Pure data, so it is
// tested without a window.

export interface GuardBox {
  cx: number;
  cy: number;
  w: number;
  h: number;
  angle: number;
}

export interface GuardScan {
  status: string;
  boxes: GuardBox[];
  /** `boxesKey` of the boxes detection produced. */
  detectedKey: string;
  /** `boxesKey` of the boxes last exported successfully, if any. */
  exportedKey: string | null;
}

/** The boxes' places, ignoring ids and differences below 0.1 px / degree, so equal boxes compare equal. */
export function boxesKey(boxes: GuardBox[]): string {
  const round = (v: number) => Math.round(v * 10) / 10;
  return boxes.map((b) => [b.cx, b.cy, b.w, b.h, b.angle].map(round).join(",")).join(";");
}

export interface CloseAssessment {
  /** Approved scans whose crops have never been exported. */
  neverExported: number;
  /** Approved scans edited after they were exported, so the files on disk are out of date. */
  changedSinceExport: number;
  /** Scans the user changed but has not approved; an export skips them. */
  editedUnapproved: number;
  /** An export is running; closing now can leave half-written images. */
  exporting: boolean;
  /** Whether closing should ask first: only while an export is running. */
  needsConfirm: boolean;
}

export function assessClose(scans: GuardScan[], exporting: boolean): CloseAssessment {
  let neverExported = 0;
  let changedSinceExport = 0;
  let editedUnapproved = 0;
  for (const scan of scans) {
    const key = boxesKey(scan.boxes);
    if (scan.status === "approved") {
      if (scan.boxes.length === 0) continue; // nothing to export
      if (scan.exportedKey === null) neverExported++;
      else if (scan.exportedKey !== key) changedSinceExport++;
    } else if (scan.status === "pending" || scan.status === "flagged") {
      if (key !== scan.detectedKey && scan.exportedKey !== key) editedUnapproved++;
    }
  }
  return {
    neverExported,
    changedSinceExport,
    editedUnapproved,
    exporting,
    needsConfirm: exporting,
  };
}
