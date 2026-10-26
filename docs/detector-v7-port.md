# Detector port notes

Notes on porting ScanSplitter's v3, v5 and v7 detectors to Rust. Record every deviation from the Python behaviour here, with the reason.

Reference: `reference/ScanSplitter` (scansplitter 0.8.0).

## Corrections to OXICUT_PLAN.md (found reading scansplitter 0.8.0)

- There is no `detector_v3.py` / `detector_v5.py`. v3, v4 and v5 all live in `src/scansplitter/detector.py`; v7 is `detector_v7.py`. Dispatch is in `detection.py`.
- v3 detection resolution is **1800 px** long side (`detection_max_dimension=1800`), not ~1500.
- `detect_photos_v3` defaults to `inset=10` (source px), and `detect_regions` does not override it, so the CLI and app get a 10 px inset in v3. v7's contract defaults to `inset=0`.
- Default area filter: `min_area_ratio=0.02`, `max_area_ratio=0.80`.

## v3

Source: `detector.py` lines 15-545 (`detect_photos_v3` and the `_v3_*` helpers, plus `_refine_rect_to_edges`).

### Pipeline

1. RGB -> resize with `INTER_AREA` so the long side is <= 1800 px.
2. `COLOR_RGB2LAB` on 8-bit (OpenCV scaling: L in 0..255, a/b offset by 128), as float32.
3. **Background colours** (`_v3_background_colors`): sample on a stride so ~30k pixels, k-means k=5 (kmeans++, 3 attempts, 30 iter / eps 0.5, RNG seed 0). Primary = argmax(count * clip(L/255, 0.15, 1)^2). Also keep clusters with L >= primary.L - 55 and ab distance <= 24 that cover >= 1.5% of a 6% outer border band, plus any cluster that >= 3 of the 4 corner pixels map to.
4. `delta` = min Lab distance to any background colour. Foreground = `delta > max(18, Otsu(clip(delta, 0, 255) as u8))`.
5. Density = box filter of foreground, window `max(11, round(0.010 * min_side)) | 1`; mask = density > 0.16.
6. Morph open (ellipse, `max(5, round(0.010 * min_side)) | 1`), then close (rect, `max(5, round(0.008 * min_side)) | 1`, 2 iterations).
7. External contours -> `minAreaRect`; keep if area within [min, max] ratio, contour fill of bounding box >= 0.32, aspect <= 5.
8. Merge fragments overlapping >= 8% (of the smaller) into their joint `minAreaRect` unless it exceeds max area; repeat to fixpoint.
9. Dedupe by area descending, dropping IoU > 0.55.
10. **Gutter split** (`_v3_split_on_gutter`): only if aspect >= 1.8. Deskew the foreground mask into the rect, take the mean profile along the long axis, blur width 7, find the minimum in the middle 25..75%. Split if min <= 0.08 and both flanks (12% of length, >= 5 px) have density >= 0.16 and both halves pass min area.
11. **Angle snap** (`_v3_snap_angle_to_long_edges`): Canny(35, 110) on 5x5-blurred gray patch (bbox + 15% short side margin), `HoughLinesP`, angles folded mod 90 within ±18° of the rect angle, length²-weighted histogram (0.5° bins), weighted mean within ±2° of the peak; needs total weight >= min_len².
12. **Edge refine** (`_refine_rect_to_edges`, margin ratio 0.12 here): deskew a gray patch, 3x3 blur, mean |Sobel| profiles per axis, move each side to the strongest peak within ±margin if peak >= 8 and >= median + max(6, 4 * MAD).
13. Scale back to source px, area-ratio filter again, apply `padding - inset`, normalise so width >= height (angle += 90 otherwise), sort by `(y // 100, x)`.

### OpenCV primitives to reimplement (no `opencv` crate)

`resize INTER_AREA`, `cvtColor` RGB->Lab and RGB->Gray (OpenCV coefficients), `kmeans` (kmeans++), Otsu threshold, `boxFilter`, ellipse/rect morphology, `findContours RETR_EXTERNAL`, `contourArea`, `boundingRect`, `minAreaRect`, `boxPoints`, `rotatedRectangleIntersection`, `warpAffine` (linear + nearest, inverse map, replicate/constant border), `GaussianBlur`, `Sobel`, `Canny`, `HoughLinesP`. `imageproc` covers some of these (contours, `min_area_rect`, Canny, Otsu, morphology, Sobel, Gaussian blur), but its outputs are not guaranteed to match OpenCV bit for bit.

### Parity risks (affect the ±2 px / ±0.5° target)

- **k-means** depends on OpenCV's RNG (seed 0) and kmeans++ seeding. We can't reproduce that RNG exactly. Plan: deterministic seeding of our own, then check that the *selected background colours* match closely on the fixtures. Small centre differences should only move `delta` slightly.
- **`HoughLinesP`** is randomised in OpenCV (it samples points in random order). The snapped angle is a weighted mean, so it should be stable, but individual line segments won't match. Possible substitute: deterministic standard Hough + segment extraction, if the angle stays within ±0.5°.
- **Resize, Lab and Gaussian kernels** need OpenCV's exact rounding/coefficients, or small threshold-boundary differences will move the contours.

### Deviations and parity checks

- **Resize (`resize.rs`)**: `INTER_AREA` reimplemented (integer block average and fractional coverage weights). Checked against cv2 on four random 8-bit RGB inputs including 4000x3000 -> 1800x1350 and 701x503 -> 233x167: bit-identical (max diff 0).
- **Lab (`color.rs`)**: OpenCV's 8-bit `RGB2Lab_b` reimplemented with its integer pipeline (gamma table, cube-root table, fixed-point matrix). Checked against cv2 on all 2^24 RGB colours: L is exact; `a`/`b` are exact except about 1,100 very dark blue colours that are one level off (cause not found; the cube-root table must be built in double precision to get L exact). An earlier float implementation was off by 1-2 on ~10% of pixels, which was enough to change cluster choice.
- **k-means (`kmeans.rs`)**: *not* a stand-in after all. OpenCV's `cv::RNG` (multiply-with-carry), `generateCentersPP` (3 trials per step), Lloyd loop and empty-cluster handling are ported, seeded like `cv2.setRNGSeed(0)`. With identical input pixels it reproduces Python's background clusters exactly (111 of 114 fixtures exact, worst centre gap 1.25 Lab units, cluster sets always equal). This supersedes the earlier plan to use our own deterministic seeding. Fused multiply-add in the distance loop made no difference.
- **JPEG decoding**: the `image` crate's JPEG decoder (zune-jpeg) is not bit-identical to PIL's libjpeg: 2-6% of channel values differ by 1-2 levels on the fixtures. That alone shifts k-means on some images, so parity tests read PIL-decoded pixels (`reference/decoded/*.png`, written by `tools/dump_v3_reference.py`). PNG and TIFF, the formats real scans use, decode identically. Expect small box differences on JPEG inputs; check them separately from the stage-by-stage parity.
- **Mask, morphology, contours (`mask.rs`, `contours.rs`, `candidates.rs`)**: box filter, ellipse/rect morphology and `findContours(RETR_EXTERNAL)` are written from scratch, no `imageproc`. Against Python on the 114 fixtures (our own foreground as input): mask pixel counts exact on 107, contours (area, bounding box, fitted rectangle within 0.05) exact on 110, final candidates exact on 112. Every miss is an image whose foreground already differed from Python's by a few pixels; the low-contrast one by 10%.
  - Box filter: counts are summed exactly in integers, then scaled in double and stored as `f32` like OpenCV, with `BORDER_REFLECT_101`; this matters because a density of exactly 0.16 is reachable.
  - Morphology treats pixels outside the image as neutral, like OpenCV's default border.
  - Contours: Moore-neighbour border following from each blob's first raster pixel. Blobs nested inside another blob's hole are skipped, as `RETR_EXTERNAL` does. Points are not compressed (`CHAIN_APPROX_SIMPLE` only drops collinear points, so area, bounding box and hull are unchanged).
  - `min_area_rect` is a hull plus a scan over edge directions in `f64`; OpenCV uses `f32`, so rectangles agree to about 0.03 px. This OpenCV build reports `minAreaRect` angles in (-90, 0]; ours is the direction of the edge a side lies along. Compare with `RotatedRect::canonical()` (width >= height, angle mod 180). A near-square's angle is only meaningful modulo 90.
- **Fragment merge, dedupe, gutter split**: `merge.rs`, `split.rs`, `warp.rs`. Rotated-rectangle intersection is Sutherland-Hodgman instead of OpenCV's own routine; areas agree to ~1e-4. Each stage, fed Python's output of the previous stage, matches on all 114 fixtures (worst rectangle difference 0.0004 px for merge, 0 for split). `warp_nearest` reproduces OpenCV's fixed-point (1/1024 px) arithmetic exactly, which is what makes the split exact.
- **Angle snap (`snap.rs`, `filter.rs`, `canny.rs`, `hough.rs`)**: Gaussian 5x5 (sigma 0 uses the fixed `[1 4 6 4 1]/16` kernel), Canny (L1 gradient, 15-bit fixed-point non-maximum suppression, hysteresis) and `HoughLinesP` are all bit-identical to OpenCV: edge maps equal pixel for pixel and the same line segments in the same order. `HoughLinesP` looks random but seeds `cv::RNG(-1)`, so it is deterministic and ported exactly. All 320 snapped rectangles get Python's exact angle.
- **OpenCV version matters**: ScanSplitter pins opencv-python-headless 5.0.0.93, which differs from OpenCV 4 in places we depend on. (1) `RGB2GRAY` is 15-bit `9798/19235/3735` (4: 14-bit `4899/9617/1868`); the old weights were off by one on 0.3% of colours, enough to change the snapped angle on 20 of 320 rectangles. (2) `RotatedRect::points` computes all four corners from separate expressions (4 mirrored two through the centre), which changes `floor(boundingRect)` for rectangles at exactly +-90 degrees; see `RotatedRect::corners_f32`. If ScanSplitter's pin moves, re-run `tools/dump_v3_reference.py` and the golden tests.- **Side refinement (`refine.rs`)**: exact on all 320 fixture rectangles. The 3x3 blur and the reflect-border Sobel column/row profiles are bit-identical to OpenCV. The deskew warp is `INTER_LINEAR`, and **OpenCV 5 evaluates linear warps in floating point, not the 1/32-pixel fixed point of OpenCV 4**: probed with a `[0, 255]` image, the output is `round(255 * fraction)` at full resolution. Our model (double coordinates from the `float32` matrix, `f32` blend, round half to even) differs from cv2 on about 0.016% of pixels by 1 level, which does not change any rectangle. `INTER_NEAREST` (used by the gutter split) *is* still fixed point and exact.- **`minAreaRect` and `findContours` conventions (OpenCV 5)**: contours come back in *reverse* raster order, and `minAreaRect` always reports its angle in `[-90, 0)`, swapping width and height when that takes an odd number of quarter turns (an axis-aligned box is always `-90`, never `0`). These matter: the gutter split, angle snap and refinement all behave differently on equivalent descriptions of the same rectangle. `RotatedRect::opencv_normalized` and `find_external_contours` reproduce both, which took the images diverging at some stage from 10 to 7 and the final boxes within tolerance from 109 to 111 of 114.
- **Float32 rounding in `minAreaRect` (known gap)**: OpenCV works in `f32`, so a rectangle whose exact centre is `204.5` comes back as `204.4999695`. Downstream that can tip a `floor`/`round` by a pixel (`scansplitter-08`). `geometry::rotating_calipers` is a faithful `f32` port of OpenCV's routine and reproduces centre and sides bit for bit in ~76% of contours, but OpenCV's own convex-hull routine fixes the vertex order and tie-breaks and that is not reproduced, so using it made end-to-end parity worse (many images diverged at the candidates stage). `min_area_rect` therefore stays with exact arithmetic and `rotating_calipers` is kept, unused, for a later attempt.
- **End-to-end parity (identical pixels)**: all 114 fixtures give the same number of boxes as Python and 111 are within 2 px / 0.5 degrees. The three outside: `low-contrast-02` and `tight-gutters-02` (their foreground already differs from Python's by a few pixels, in a low-contrast scene) and `scansplitter-08` (the float32 effect above).
- **JPEG inputs**: running the original `.jpg` fixtures through our own decoder (zune-jpeg, which is not bit-identical to libjpeg) matches Python on far fewer images (many differ by tens of pixels, some by hundreds; a few get a different box count). That is sensitivity of the detector to small pixel differences on hard test images, not a porting error; PNG/TIFF decode identically. Do not use JPEG results to judge the port.- **16-bit input**: PIL's `convert("RGB")` keeps the high byte (`v >> 8`); the `image` crate rounds. `to_detection_rgb` truncates like PIL. Only the detection copy is 8-bit; crops keep the source bit depth. 16-bit greyscale TIFFs are not verified against PIL (it treats `I;16` differently) and are untested.

## v5

_Not started._

## v7

_Not started._

## Accuracy against ground truth (v3)

ScanSplitter ships ground-truth boxes (`benchmarks/manifest.json`; target = "inner photographic image area; exclude paper borders, tape, corners, shadows"). Measured on `scansplitter-05` (six prints on an album spread), our v3 equals Python's v3, and:

- **Inset.** ScanSplitter's default of 10 px per side makes every box about 18 px too small in width or height. With inset 0, the boxes that are found correctly are within about 2 px of the truth. OxiCut therefore defaults to 0 (the library `DetectOptions` default stays 10 for parity with the port tests).
- **Border and shadow.** On four of the six prints v3 puts the right edge about 21 px too far out (and on one print the bottom edge about 29 px), because the side refinement locks onto the outer paper edge or its drop shadow. This is the white-border case the plan calls out, and it is what v7's per-side inner-edge tightening is for. It is a property of v3, not of the port.

Reproduce with `oxicut detect <scan> --inset 0` and the manifest's rectangles.

## Benchmark parity (v3)

`oxicut bench` ports ScanSplitter's scoring (`benchmark_metrics.py`: Hungarian match on rotated IoU, box quality, F1@0.50 and F1@0.85, coverage, tightness). Run with ScanSplitter's own default trim (`--inset 10`) it reproduces the published v3 numbers:

| Suite | Metric | OxiCut | ScanSplitter (published/run) |
|---|---|---:|---:|
| scansplitter (10 scenes) | box quality | 50.6% | 50.6% |
| | F1@0.50 / F1@0.85 | 57.5% / 42.5% | 57.5% / 42.5% |
| robust-dev (47) | box quality | 69.4% | 69.7% |
| | F1@0.50 / F1@0.85 | 79.7% / 46.5% | 79.7% / 47.1% |
| robust-holdout (47) | box quality | 73.0% | 73.0% |
| | F1@0.50 / F1@0.85 | 84.8% / 43.3% | 85.7% / 43.8% |

The small gaps (and a few points of tightness) are JPEG-decoder differences on hard images. The target to beat is v7: ScanSplitter reports 88.8% box quality on the robust holdout.