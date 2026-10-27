// Geometry for the loupe that follows a marquee handle while it is dragged.
// Everything is in scan pixels; angles are degrees, clockwise on screen (as Fabric and the engine use).

export interface LoupeBox {
  cx: number;
  cy: number;
  w: number;
  h: number;
  angle: number;
}

export type Point2 = { x: number; y: number };

/** Which side of the box a Fabric control sits on, as signs along the box's own axes. `null` for rotation. */
export function handleOffset(corner: string): Point2 | null {
  const signs: Record<string, [number, number]> = {
    tl: [-1, -1],
    tr: [1, -1],
    bl: [-1, 1],
    br: [1, 1],
    ml: [-1, 0],
    mr: [1, 0],
    mt: [0, -1],
    mb: [0, 1],
  };
  const found = signs[corner];
  return found ? { x: found[0], y: found[1] } : null;
}

function rotate(x: number, y: number, angle: number): Point2 {
  const t = (angle * Math.PI) / 180;
  return { x: x * Math.cos(t) - y * Math.sin(t), y: x * Math.sin(t) + y * Math.cos(t) };
}

/** The four corners of the rotated box: top-left, top-right, bottom-right, bottom-left. */
export function boxCorners(box: LoupeBox): Point2[] {
  return [
    [-1, -1],
    [1, -1],
    [1, 1],
    [-1, 1],
  ].map(([sx, sy]) => {
    const r = rotate((sx * box.w) / 2, (sy * box.h) / 2, box.angle);
    return { x: box.cx + r.x, y: box.cy + r.y };
  });
}

/** Where a handle sits on the box, or `null` if it is not a resize handle. */
export function handlePoint(box: LoupeBox, corner: string): Point2 | null {
  const sign = handleOffset(corner);
  if (!sign) return null;
  const r = rotate((sign.x * box.w) / 2, (sign.y * box.h) / 2, box.angle);
  return { x: box.cx + r.x, y: box.cy + r.y };
}

/** Square of `size` screen pixels centred on `centre` shows `size / magnification` scan pixels. */
export function sourceSquare(centre: Point2, size: number, magnification: number) {
  const half = size / 2 / magnification;
  return { x: centre.x - half, y: centre.y - half, side: half * 2 };
}

export type LoupePlace = "top-left" | "top-right" | "bottom-left" | "bottom-right";

/**
 * The stage corner each of four screen points belongs in, in the order of `points`: the two highest go
 * on top and the two lowest at the bottom, each pair left to right by x. (Ranking by quadrant around the
 * centre would give a thin box at 45 degrees two corners in the same quadrant.)
 */
export function cornerPlaces(points: Point2[]): LoupePlace[] {
  const ranked = points.map((p, index) => ({ p, index })).sort((a, b) => a.p.y - b.p.y || a.p.x - b.p.x);
  const places = new Array<LoupePlace>(points.length);
  const rows: [typeof ranked, LoupePlace, LoupePlace][] = [
    [ranked.slice(0, 2), "top-left", "top-right"],
    [ranked.slice(2), "bottom-left", "bottom-right"],
  ];
  for (const [row, left, right] of rows) {
    row.sort((a, b) => a.p.x - b.p.x);
    places[row[0].index] = left;
    places[row[1].index] = right;
  }
  return places;
}
