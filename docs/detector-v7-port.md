# Detector port notes

Notes on porting ScanSplitter's v3, v5 and v7 detectors to Rust. Record every deviation from the Python behaviour here, with the reason.

Reference: `reference/ScanSplitter` (scansplitter 0.8.0).

## Corrections to OXCUT_PLAN.md (found reading scansplitter 0.8.0)

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
- **16-bit input**: PIL's `convert("RGB")` keeps the high byte (`v >> 8`); the `image` crate rounds. `to_detection_rgb` truncates like PIL. Only the detection copy is 8-bit; crops keep the source bit depth. 16-bit greyscale TIFFs are not verified against PIL (it treats `I;16` differently) and are untested.

## v5

_Not started._

## v7

_Not started._
