// Telling when a scan was found with other detection settings than the ones now chosen, so the screen
// can point at Re-detect. Pure, so the rules are tested without a window.

import type { DetectSettings } from "./api";

/** The settings that change what is found, as stable text. Export options, theme and the rest are not in it. */
export function detectKey(s: DetectSettings): string {
  return JSON.stringify([s.mode, s.minAreaPercent, s.maxAreaPercent, s.insetPx, s.cutTo, s.autoOrient]);
}

export interface StaleCheck {
  status: string;
  /** The `detectKey` of the settings the scan was found with; empty when not known (an older project). */
  detectedWith: string;
  /** `boxesKey` of what detection produced, and of the boxes now. */
  detectedKey: string;
  boxesKey: string;
}

/**
 * The scan was found with other settings than `currentKey`. A scan still being found, one that failed, one
 * from before this was remembered, and one the person has already fixed by hand (re-detecting would throw
 * their work away) are left alone.
 */
export function isOutOfDate(scan: StaleCheck, currentKey: string): boolean {
  if (scan.status === "queued" || scan.status === "detecting" || scan.status === "error") return false;
  if (scan.detectedWith === "" || scan.detectedWith === currentKey) return false;
  return scan.boxesKey === scan.detectedKey;
}
