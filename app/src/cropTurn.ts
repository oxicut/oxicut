// Turning a crop by 90 degrees is turning its box: the same region with the angle changed by 90 and
// width and height swapped. The box on the scan looks the same afterwards, but the exported crop comes
// out turned. The signs match `crops_are_straightened_and_turning_a_box_turns_its_crop` in
// `app/src-tauri/src/engine.rs`: clockwise lowers the angle, counter-clockwise raises it.

export interface TurnableBox {
  cx: number;
  cy: number;
  w: number;
  h: number;
  angle: number;
}

/** An angle in degrees brought into [-180, 180). */
export function normalizeAngle(angle: number): number {
  const wrapped = (((angle + 180) % 360) + 360) % 360;
  return wrapped - 180;
}

/** The box that gives a crop turned a quarter clockwise (`"right"`) or counter-clockwise (`"left"`). */
export function turnBox<T extends TurnableBox>(box: T, direction: "left" | "right"): T {
  const angle = normalizeAngle(box.angle + (direction === "right" ? -90 : 90));
  return { ...box, w: box.h, h: box.w, angle };
}
