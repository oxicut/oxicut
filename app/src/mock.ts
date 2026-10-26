// Placeholder data for the UI preview. Replaced by real detection results in Phase 3.

export type Flag = "no-item" | "touches-edge" | "odd-aspect" | "overlap";
export type ScanStatus = "approved" | "flagged" | "pending";

/** A rotated rectangle in source pixels (OpenCV convention, angle in degrees). */
export interface Box {
  cx: number;
  cy: number;
  w: number;
  h: number;
  angle: number;
}

export interface Scan {
  id: number;
  name: string;
  status: ScanStatus;
  flags: Flag[];
  boxes: Box[];
}

export const SCAN_WIDTH = 2550;
export const SCAN_HEIGHT = 3507;

export const FLAG_LABELS: Record<Flag, string> = {
  "no-item": "No item found",
  "touches-edge": "Touches scan edge",
  "odd-aspect": "Unusual aspect ratio",
  overlap: "Overlapping boxes",
};

const box = (cx: number, cy: number, w: number, h: number, angle: number): Box => ({
  cx,
  cy,
  w,
  h,
  angle,
});

export function mockScans(): Scan[] {
  return [
    {
      id: 1,
      name: "postcards_front_001.tif",
      status: "approved",
      flags: [],
      boxes: [box(1275, 950, 1700, 1150, 0.8), box(1275, 2500, 1700, 1150, -0.5)],
    },
    {
      id: 2,
      name: "postcards_back_001.tif",
      status: "approved",
      flags: [],
      boxes: [box(1275, 960, 1700, 1150, -0.4), box(1275, 2490, 1700, 1150, 0.6)],
    },
    {
      id: 3,
      name: "album_p03.jpg",
      status: "flagged",
      flags: ["overlap"],
      boxes: [box(900, 1000, 1300, 1000, 3), box(1500, 1250, 1300, 1000, -2)],
    },
    {
      id: 4,
      name: "prints_0007.tif",
      status: "flagged",
      flags: ["touches-edge"],
      boxes: [box(1275, 1100, 1900, 1300, 1.2), box(1275, 3180, 1500, 640, 0)],
    },
    { id: 5, name: "prints_0008.tif", status: "flagged", flags: ["no-item"], boxes: [] },
    {
      id: 6,
      name: "prints_0009.tif",
      status: "pending",
      flags: [],
      boxes: [
        box(700, 900, 1000, 1400, -1.5),
        box(1850, 900, 1000, 1400, 2.1),
        box(1275, 2650, 2000, 1250, 0.3),
      ],
    },
    {
      id: 7,
      name: "holiday_1974_a.png",
      status: "pending",
      flags: [],
      boxes: [box(1275, 1750, 1600, 2200, 7)],
    },
    {
      id: 8,
      name: "holiday_1974_b.png",
      status: "flagged",
      flags: ["odd-aspect"],
      boxes: [box(1275, 1750, 2300, 380, 0)],
    },
  ];
}

export interface ModelInfo {
  id: string;
  name: string;
  detail: string;
  sizeMb: number;
  /** Which detection modes need this model. */
  neededBy: string;
}

export const MODELS: ModelInfo[] = [
  {
    id: "mobilesam-encoder",
    name: "MobileSAM image encoder",
    detail: "Segments each scan once; cached per image",
    sizeMb: 28,
    neededBy: "v5, v7",
  },
  {
    id: "mobilesam-decoder",
    name: "MobileSAM prompt decoder",
    detail: "Turns box prompts into masks",
    sizeMb: 15,
    neededBy: "v5, v7",
  },
];
