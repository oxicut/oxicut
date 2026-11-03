// How well does a detector find what a person marked by hand?
//
//   oxicut detect scan.jpg --mode v3 | node tools/compare-project.mjs scan.truth.oxicut
//
// Reads the boxes `oxicut detect` printed (on standard input) and the boxes of the first scan in an
// `.oxicut` project file (the ones fixed by hand), and prints for each hand-fixed box the best overlap
// (intersection over union of the rotated rectangles) with any detected box. Handy for checking a change
// against a real scan: a project a person has corrected is a small answer key.
import fs from "node:fs";

const file = process.argv[2];
if (!file) {
  console.error("usage: oxicut detect <scan> | node tools/compare-project.mjs <project.oxicut>");
  process.exit(2);
}
const truth = JSON.parse(fs.readFileSync(file, "utf8").replace(/^﻿/, "")).scans[0].boxes;
const found = JSON.parse(fs.readFileSync(0, "utf8")).regions;

const corners = ({ cx, cy, w, h, angle }) => {
  const a = (angle * Math.PI) / 180;
  const c = Math.cos(a);
  const s = Math.sin(a);
  return [[-w / 2, -h / 2], [w / 2, -h / 2], [w / 2, h / 2], [-w / 2, h / 2]].map(([x, y]) => [cx + x * c - y * s, cy + x * s + y * c]);
};
const signedArea = (p) => p.reduce((t, [x, y], i) => t + (x * p[(i + 1) % p.length][1] - p[(i + 1) % p.length][0] * y), 0) / 2;
const counterClockwise = (p) => (signedArea(p) < 0 ? [...p].reverse() : p);

/** Sutherland-Hodgman: `subject` clipped to the convex polygon `clipper` (both counter-clockwise). */
function clip(subject, clipper) {
  let out = subject;
  for (let i = 0; i < clipper.length && out.length; i++) {
    const a = clipper[i];
    const b = clipper[(i + 1) % clipper.length];
    const inside = (p) => (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) >= 0;
    const cross = (p, q) => {
      const dx = q[0] - p[0];
      const dy = q[1] - p[1];
      const ex = b[0] - a[0];
      const ey = b[1] - a[1];
      const t = ((a[0] - p[0]) * ey - (a[1] - p[1]) * ex) / (dx * ey - dy * ex);
      return [p[0] + t * dx, p[1] + t * dy];
    };
    const input = out;
    out = [];
    input.forEach((p, k) => {
      const q = input[(k + 1) % input.length];
      if (inside(p)) {
        out.push(p);
        if (!inside(q)) out.push(cross(p, q));
      } else if (inside(q)) {
        out.push(cross(p, q));
      }
    });
  }
  return out;
}

function iou(a, b) {
  const pa = counterClockwise(corners(a));
  const pb = counterClockwise(corners(b));
  const inter = clip(pa, pb);
  const common = inter.length ? Math.abs(signedArea(inter)) : 0;
  return common / (Math.abs(signedArea(pa)) + Math.abs(signedArea(pb)) - common);
}

console.log(`hand-fixed ${truth.length} boxes, detected ${found.length}`);
let sum = 0;
truth.forEach((t, i) => {
  let best = { value: 0, index: -1 };
  found.forEach((f, j) => {
    const value = iou(t, f);
    if (value > best.value) best = { value, index: j };
  });
  sum += best.value;
  const f = found[best.index];
  console.log(
    `  box ${i + 1} (centre ${Math.round(t.cx)},${Math.round(t.cy)} ${Math.round(t.w)}x${Math.round(t.h)}): best overlap ${best.value.toFixed(2)}` +
      (f ? `  <- ${Math.round(f.cx)},${Math.round(f.cy)} ${Math.round(f.w)}x${Math.round(f.h)} @${f.angle.toFixed(1)}` : ""),
  );
});
const stray = found.filter((f) => truth.every((t) => iou(t, f) < 0.3)).length;
console.log(`mean best overlap ${(sum / truth.length).toFixed(2)}; detected boxes that match no hand-fixed box: ${stray}`);
