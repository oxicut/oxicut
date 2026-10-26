// Undo / redo bookkeeping for one scan's boxes. Pure data, so it can be tested without a window.

/** Snapshots of some state, oldest first. Plain fields (no `#private`) so Vue can make them reactive. */
export class History<T> {
  past: T[] = [];
  future: T[] = [];

  limit: number;

  constructor(limit = 100) {
    this.limit = limit;
  }

  get canUndo(): boolean {
    return this.past.length > 0;
  }

  get canRedo(): boolean {
    return this.future.length > 0;
  }

  /** The state just before a new edit. A new edit ends whatever could have been redone. */
  record(before: T) {
    this.past.push(before);
    if (this.past.length > this.limit) this.past.shift();
    this.future = [];
  }

  /** Step back: returns the state to restore, or `undefined` if there is nothing to undo. */
  undo(current: T): T | undefined {
    const previous = this.past.pop();
    if (previous === undefined) return undefined;
    this.future.push(current);
    return previous;
  }

  /** Step forward again after an undo. */
  redo(current: T): T | undefined {
    const next = this.future.pop();
    if (next === undefined) return undefined;
    this.past.push(current);
    return next;
  }

  clear() {
    this.past = [];
    this.future = [];
  }
}

export interface Rectangle {
  id: number;
  cx: number;
  cy: number;
  w: number;
  h: number;
  angle: number;
}

/** Whether two box lists are the same boxes in the same places (within `tolerance` pixels/degrees). */
export function sameBoxes(a: Rectangle[], b: Rectangle[], tolerance = 0.01): boolean {
  if (a.length !== b.length) return false;
  return a.every((box, i) => {
    const other = b[i];
    return (
      box.id === other.id &&
      Math.abs(box.cx - other.cx) <= tolerance &&
      Math.abs(box.cy - other.cy) <= tolerance &&
      Math.abs(box.w - other.w) <= tolerance &&
      Math.abs(box.h - other.h) <= tolerance &&
      Math.abs(box.angle - other.angle) <= tolerance
    );
  });
}
