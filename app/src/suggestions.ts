// "Try another cut": plain words for how a suggested box differs from the one it would replace. Pure, so
// the wording is tested without a window. No numbers on screen: what a person can see is closer, wider,
// straighter, moved.

import type { Box } from "./api";

/** An area change smaller than this is not worth a word. */
const AREA_CHANGE = 0.03;
/** A change in the sides' proportions smaller than this (as a ratio) is the same shape. */
const SHAPE_CHANGE = 0.04;
/** Degrees under which a tilt is the same tilt. */
const TILT_CHANGE = 0.5;
/** A move smaller than this share of the longer side is the same place. */
const MOVE_CHANGE = 0.015;

/** Degrees away from the nearest level or upright position (a multiple of 90). */
const offLevel = (angle: number) => {
  const m = ((angle % 90) + 90) % 90;
  return Math.min(m, 90 - m);
};
const normalise = (a: number) => ((((a + 180) % 360) + 360) % 360) - 180;

/** How `next` differs from `current`, as a short phrase starting with a capital ("Cuts closer and straighter"). */
export function describeSuggestion(current: Box, next: Box): string {
  const parts: string[] = [];

  const area = (next.w * next.h) / Math.max(1, current.w * current.h);
  if (area < 1 - AREA_CHANGE) parts.push("cuts closer");
  else if (area > 1 + AREA_CHANGE) parts.push("cuts wider");

  const shape = Math.abs(Math.log((next.w / Math.max(1, next.h)) / (current.w / Math.max(1, current.h))));
  if (shape > SHAPE_CHANGE) parts.push("a different shape");

  const tilt = Math.abs(normalise(next.angle - current.angle));
  if (tilt >= TILT_CHANGE) {
    // Nearer to level (a multiple of 90) than before, or just tilted differently.
    parts.push(offLevel(next.angle) < offLevel(current.angle) ? "straighter" : "tilted differently");
  }

  const move = Math.hypot(next.cx - current.cx, next.cy - current.cy) / Math.max(1, Math.max(current.w, current.h));
  if (parts.length === 0 && move > MOVE_CHANGE) parts.push("moved a little");

  if (parts.length === 0) return "A different cut";
  const text = parts.length === 1 ? parts[0] : `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
  return text[0].toUpperCase() + text.slice(1);
}
