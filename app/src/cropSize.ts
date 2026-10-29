// "Cut closer" and "Cut wider" in the larger view of a crop: every edge of the box moves in or out a
// little, keeping its centre and angle. Pure, so the rules are tested without a window.

import { boxCorners } from "./loupe.ts";
import type { TurnableBox } from "./cropTurn.ts";

/** How far each edge moves per click, as a fraction of the box's shorter side. */
export const EDGE_STEP = 0.01;

/** No crop gets smaller than this (in scan pixels) on either side. */
export const SMALLEST_SIDE = 16;

export type SizeChange = "closer" | "wider";

/** Pixels each edge moves per click for a box of this size (at least one). */
export function edgeStep(box: Pick<TurnableBox, "w" | "h">): number {
  return Math.max(1, Math.round(Math.min(box.w, box.h) * EDGE_STEP));
}

/**
 * The box after one click, or `null` when that is not possible: `"closer"` would leave a side under
 * `SMALLEST_SIDE`, `"wider"` would reach past the edge of the scan (where there is nothing to cut).
 */
export function resizeBox<T extends TurnableBox>(box: T, change: SizeChange, scan: { width: number; height: number }): T | null {
  const step = edgeStep(box);
  const grow = change === "wider" ? 2 * step : -2 * step;
  const next = { ...box, w: box.w + grow, h: box.h + grow };
  if (change === "closer") return Math.min(next.w, next.h) >= SMALLEST_SIDE ? next : null;
  const inside = boxCorners(next).every((p) => p.x >= -0.5 && p.y >= -0.5 && p.x <= scan.width + 0.5 && p.y <= scan.height + 0.5);
  return inside ? next : null;
}
