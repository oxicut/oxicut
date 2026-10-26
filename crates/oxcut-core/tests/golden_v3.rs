//! Compare stages of the Rust v3 port with ScanSplitter's Python output.
//!
//! Golden values live in `benchmarks/golden/v3.json` (made by
//! `tools/dump_v3_reference.py`); the fixture images are in the gitignored
//! `reference/ScanSplitter` checkout. Tests skip when the checkout is missing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use oxcut_core::background::background_colors;
use oxcut_core::candidates::candidate_rects;
use oxcut_core::color::{rgb_to_gray, rgb_to_lab};
use oxcut_core::contours::{contour_info, find_external_contours};
use oxcut_core::detect::{DetectOptions, detect_v3};
use oxcut_core::foreground::foreground;
use oxcut_core::geometry::RotatedRect;
use oxcut_core::io::{load_image, to_detection_rgb};
use oxcut_core::mask::{Element, close, close_size, density_mask, density_window, open, open_size};
use oxcut_core::merge::{deduplicate, merge_fragments};
use oxcut_core::refine::refine_rect_to_edges;
use oxcut_core::resize::{DETECTION_MAX_DIMENSION, for_detection};
use oxcut_core::snap::snap_angle_to_long_edges;
use oxcut_core::split::split_on_gutter;
use serde::Deserialize;

#[derive(Deserialize)]
struct Golden {
    images: BTreeMap<String, GoldenImage>,
}

// snapped, efined and egions are read by the later-stage tests.
#[allow(dead_code)]
#[derive(Deserialize)]
struct GoldenImage {
    detection_size: [u32; 2],
    backgrounds_lab: Vec<[f32; 3]>,
    otsu: u8,
    foreground_count: usize,
    density_count: usize,
    opened_count: usize,
    mask_count: usize,
    contours: Vec<GoldenContour>,
    candidates: Vec<GoldenRect>,
    merged: Vec<GoldenRect>,
    deduplicated: Vec<GoldenRect>,
    split: Vec<GoldenRect>,
    snapped: Vec<GoldenRect>,
    refined: Vec<GoldenRect>,
    regions: Vec<GoldenRect>,
}

#[derive(Deserialize)]
struct GoldenContour {
    area: f64,
    bbox: [i32; 4],
    rect: GoldenRect,
}

#[derive(Deserialize, Clone, Copy)]
struct GoldenRect {
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    angle: f64,
}

impl GoldenRect {
    fn rect(self) -> RotatedRect {
        RotatedRect::new(self.cx, self.cy, self.w, self.h, self.angle)
    }

    fn canonical(self) -> RotatedRect {
        RotatedRect::new(self.cx, self.cy, self.w, self.h, self.angle).canonical()
    }
}

/// Largest difference between two canonical rectangles (centre, sides, angle).
fn rect_gap(a: RotatedRect, b: RotatedRect) -> f64 {
    // A near-square has no meaningful long side, so its angle is only defined
    // modulo 90 degrees.
    let period = if (a.w - a.h).abs() < 1.0 { 90.0 } else { 180.0 };
    let mut angle = (a.angle - b.angle).abs() % period;
    angle = angle.min(period - angle);
    [
        (a.cx - b.cx).abs(),
        (a.cy - b.cy).abs(),
        (a.w - b.w).abs(),
        (a.h - b.h).abs(),
        angle,
    ]
    .into_iter()
    .fold(0.0, f64::max)
}

/// Python's detection-size RGB for a fixture, as PIL decoded it.
fn detection_rgb(name: &str) -> image::RgbImage {
    let image = load_image(
        &workspace_root()
            .join("reference/decoded")
            .join(format!("{name}.png")),
    )
    .unwrap();
    for_detection(&to_detection_rgb(&image), DETECTION_MAX_DIMENSION).image
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden() -> Golden {
    let text = std::fs::read_to_string(workspace_root().join("benchmarks/golden/v3.json"))
        .expect("benchmarks/golden/v3.json is committed");
    serde_json::from_str(&text).expect("golden json parses")
}

fn reference_available() -> bool {
    let present = workspace_root()
        .join("reference/ScanSplitter/benchmarks")
        .is_dir();
    if !present {
        eprintln!("skipping: reference/ScanSplitter is not checked out");
    }
    present
}

/// Greatest per-channel gap between two sets of centres, matching each golden
/// centre to its nearest ours. `None` if the sets differ in size.
fn centre_gap(ours: &[[f32; 3]], theirs: &[[f32; 3]]) -> Option<f32> {
    if ours.len() != theirs.len() {
        return None;
    }
    let mut worst = 0f32;
    for t in theirs {
        let gap = ours
            .iter()
            .map(|o| (0..3).map(|c| (o[c] - t[c]).abs()).fold(0f32, f32::max))
            .fold(f32::INFINITY, f32::min);
        worst = worst.max(gap);
    }
    Some(worst)
}

#[test]
fn detection_size_and_background_match_python() {
    if !reference_available() {
        return;
    }
    let golden = golden();
    let mut report = Vec::new();
    for (name, expected) in &golden.images {
        // PIL-decoded pixels (see tools/dump_v3_reference.py): our JPEG decoder
        // differs from libjpeg by up to a few levels, which would blur the comparison.
        let image = load_image(
            &workspace_root()
                .join("reference/decoded")
                .join(format!("{name}.png")),
        )
        .unwrap();
        let detection = for_detection(&to_detection_rgb(&image), DETECTION_MAX_DIMENSION);
        assert_eq!(
            [detection.image.width(), detection.image.height()],
            expected.detection_size,
            "{name}: detection size"
        );

        let lab = rgb_to_lab(&detection.image);
        let ours = background_colors(&lab);
        match centre_gap(&ours, &expected.backgrounds_lab) {
            Some(gap) if gap > 3.0 => report.push((
                name.clone(),
                gap,
                Some(format!(
                    "gap {gap:.1}: ours {ours:?} vs python {:?}",
                    expected.backgrounds_lab
                )),
            )),
            Some(gap) => report.push((name.clone(), gap, None)),
            None => report.push((
                name.clone(),
                f32::NAN,
                Some(format!(
                    "ours {ours:?} vs python {:?}",
                    expected.backgrounds_lab
                )),
            )),
        }
    }

    let mismatched: Vec<_> = report.iter().filter(|r| r.2.is_some()).collect();
    let worst = report
        .iter()
        .map(|r| r.1)
        .filter(|g| g.is_finite())
        .fold(0f32, f32::max);
    let exact = report.iter().filter(|r| r.1 < 0.05).count();
    println!(
        "{} images, {exact} exact, worst centre gap {worst:.2}, {} cluster-count mismatches",
        report.len(),
        mismatched.len()
    );
    for (name, _, detail) in &mismatched {
        println!("  MISMATCH {name}: {}", detail.as_ref().unwrap());
    }
    // Exact for nearly all; the rest differ through the few dark-blue colours
    // whose 8-bit Lab is one level off OpenCV's (see docs/detector-v7-port.md).
    assert!(
        mismatched.is_empty(),
        "background cluster sets differ from Python"
    );
    assert!(worst < 2.0, "worst background centre gap {worst}");
    assert!(
        exact >= report.len() - 5,
        "only {exact} of {} exact",
        report.len()
    );
}

#[test]
fn foreground_matches_python() {
    if !reference_available() {
        return;
    }
    let golden = golden();
    let (mut exact, mut otsu_differs, mut worst_relative) = (0, 0, 0f64);
    let mut worst_same_background = 0f64;
    for (name, expected) in &golden.images {
        let lab = rgb_to_lab(&detection_rgb(name));
        let backgrounds = background_colors(&lab);
        let ours = foreground(&lab, &backgrounds);
        let count = ours.mask.iter().filter(|&&b| b).count();

        // Where our background clusters are exactly Python's, everything
        // downstream must be too; elsewhere allow the tiny drift.
        let same_background =
            centre_gap(&backgrounds, &expected.backgrounds_lab).is_some_and(|gap| gap < 0.05);
        if same_background && ours.otsu == expected.otsu && count == expected.foreground_count {
            exact += 1;
        } else {
            let relative = (count as f64 - expected.foreground_count as f64).abs()
                / expected.foreground_count.max(1) as f64;
            worst_relative = worst_relative.max(relative);
            if same_background {
                worst_same_background = worst_same_background.max(relative);
            }
            if ours.otsu != expected.otsu {
                otsu_differs += 1;
            }
            println!(
                "  {name}: otsu {} vs {}, foreground {count} vs {} (same background: {same_background})",
                ours.otsu, expected.otsu, expected.foreground_count
            );
        }
    }
    println!(
        "{} images, {exact} exact, {otsu_differs} Otsu differences, worst foreground count error {:.3}%",
        golden.images.len(),
        worst_relative * 100.0
    );
    // Images whose background clusters differ slightly can move the count a lot
    // (low-contrast scans); with identical clusters only the few dark-blue Lab
    // pixels can differ.
    assert_eq!(otsu_differs, 0, "Otsu threshold differs from Python");
    assert!(exact >= golden.images.len() - 10, "only {exact} exact");
    assert!(
        worst_same_background < 0.001,
        "foreground count off by {:.4}% despite identical background",
        worst_same_background * 100.0
    );
}

#[test]
fn mask_contours_and_candidates_match_python() {
    if !reference_available() {
        return;
    }
    let golden = golden();
    let (mut counts_exact, mut contours_exact, mut candidates_exact) = (0, 0, 0);
    let mut worst_rect_gap = 0f64;
    let mut problems = Vec::new();

    for (name, expected) in &golden.images {
        let lab = rgb_to_lab(&detection_rgb(name));
        let fg = foreground(&lab, &background_colors(&lab));
        let min_side = fg.width.min(fg.height);
        let dense = density_mask(&fg, density_window(min_side));
        let opened = open(&dense, &Element::ellipse(open_size(min_side)));
        let closed = close(&opened, &Element::rect(close_size(min_side)), 2);
        let count = |m: &image::GrayImage| m.as_raw().iter().filter(|&&v| v != 0).count();
        let ours = (count(&dense), count(&opened), count(&closed));
        let theirs = (
            expected.density_count,
            expected.opened_count,
            expected.mask_count,
        );
        if ours == theirs {
            counts_exact += 1;
        } else {
            problems.push(format!("{name}: mask counts {ours:?} vs python {theirs:?}"));
        }

        // Contours, matched by position since discovery order may differ.
        let mut our_contours: Vec<_> = find_external_contours(&closed)
            .iter()
            .map(|c| contour_info(c))
            .collect();
        our_contours.sort_by_key(|c| (c.bbox.1, c.bbox.0, c.area as i64));
        let mut their_contours: Vec<&GoldenContour> = expected.contours.iter().collect();
        their_contours.sort_by_key(|c| (c.bbox[1], c.bbox[0], c.area as i64));
        let contours_equal = our_contours.len() == their_contours.len()
            && our_contours.iter().zip(&their_contours).all(|(a, b)| {
                a.area == b.area
                    && [a.bbox.0, a.bbox.1, a.bbox.2, a.bbox.3] == b.bbox
                    // Tiny blobs have ill-defined angles; only compare real ones.
                    && (b.area < 400.0 || {
                        let gap = rect_gap(a.rect.canonical(), b.rect.canonical());
                        worst_rect_gap = worst_rect_gap.max(gap);
                        gap < 0.05
                    })
            });
        if contours_equal {
            contours_exact += 1;
        } else {
            problems.push(format!(
                "{name}: contours differ ({} vs python {})",
                our_contours.len(),
                their_contours.len()
            ));
        }

        let mut our_candidates: Vec<_> = candidate_rects(&closed, 0.02, 0.80)
            .iter()
            .map(|r| r.canonical())
            .collect();
        let their_candidates: Vec<_> = expected.candidates.iter().map(|r| r.canonical()).collect();
        let our_count = our_candidates.len();
        // Each of theirs must have a distinct counterpart of ours (order differs).
        let candidates_equal = our_count == their_candidates.len() && {
            let mut unused = std::mem::take(&mut our_candidates);
            their_candidates.iter().all(|b| {
                let best = unused
                    .iter()
                    .enumerate()
                    .map(|(i, a)| (i, rect_gap(*a, *b)))
                    .min_by(|x, y| x.1.total_cmp(&y.1));
                best.is_some_and(|(i, gap)| {
                    if gap < 1.0 {
                        worst_rect_gap = worst_rect_gap.max(gap);
                    }
                    unused.swap_remove(i);
                    gap < 0.05
                })
            })
        };
        if candidates_equal {
            candidates_exact += 1;
        } else {
            problems.push(format!(
                "{name}: candidates differ ({our_count} vs python {})",
                their_candidates.len()
            ));
        }
    }

    let n = golden.images.len();
    println!(
        "{n} images: mask counts exact {counts_exact}, contours exact {contours_exact}, candidates exact {candidates_exact}; worst rect gap {worst_rect_gap:.4}"
    );
    for problem in &problems {
        println!("  {problem}");
    }
    // The misses are images whose foreground already differed from Python's by
    // a few pixels (or by 10% on one low-contrast scan); the mask stage itself
    // is exact when given the same foreground.
    assert!(
        counts_exact >= n - 8,
        "mask counts exact on only {counts_exact}"
    );
    assert!(
        contours_exact >= n - 6,
        "contours exact on only {contours_exact}"
    );
    assert!(
        candidates_exact >= n - 3,
        "candidates exact on only {candidates_exact}"
    );
    assert!(worst_rect_gap < 0.05, "rectangle gap {worst_rect_gap}");
}

/// Match every expected rectangle to a distinct one of ours (order differs).
/// Returns the worst gap among matches, or `None` when the sets cannot match.
fn match_rects(ours: &[RotatedRect], theirs: &[GoldenRect], tolerance: f64) -> Option<f64> {
    if ours.len() != theirs.len() {
        return None;
    }
    let mut unused: Vec<RotatedRect> = ours.iter().map(|r| r.canonical()).collect();
    let mut worst = 0f64;
    for expected in theirs {
        let expected = expected.canonical();
        let (index, gap) = unused
            .iter()
            .enumerate()
            .map(|(i, r)| (i, rect_gap(*r, expected)))
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        if gap >= tolerance {
            return None;
        }
        worst = worst.max(gap);
        unused.swap_remove(index);
    }
    Some(worst)
}

#[test]
fn merge_and_dedupe_match_python() {
    if !reference_available() {
        return;
    }
    let golden = golden();
    let (mut merge_exact, mut dedupe_exact, mut worst) = (0, 0, 0f64);
    for (name, expected) in &golden.images {
        // Each stage starts from Python's output of the previous stage.
        let candidates: Vec<_> = expected.candidates.iter().map(|r| r.rect()).collect();
        let max_area = 0.80 * (expected.detection_size[0] * expected.detection_size[1]) as f64;
        let merged = merge_fragments(&candidates, max_area);
        match match_rects(&merged, &expected.merged, 0.05) {
            Some(gap) => {
                merge_exact += 1;
                worst = worst.max(gap);
            }
            None => println!(
                "  {name}: merge differs ({} vs python {})",
                merged.len(),
                expected.merged.len()
            ),
        }

        let merged_input: Vec<_> = expected.merged.iter().map(|r| r.rect()).collect();
        let deduplicated = deduplicate(&merged_input);
        match match_rects(&deduplicated, &expected.deduplicated, 0.05) {
            Some(gap) => {
                dedupe_exact += 1;
                worst = worst.max(gap);
            }
            None => println!(
                "  {name}: dedupe differs ({} vs python {})",
                deduplicated.len(),
                expected.deduplicated.len()
            ),
        }
    }
    let n = golden.images.len();
    println!(
        "{n} images: merge exact {merge_exact}, dedupe exact {dedupe_exact}, worst gap {worst:.4}"
    );
    assert_eq!(merge_exact, n, "merge differs from Python");
    assert_eq!(dedupe_exact, n, "dedupe differs from Python");
}

#[test]
fn gutter_split_matches_python() {
    if !reference_available() {
        return;
    }
    let golden = golden();
    let (mut exact, mut worst) = (0, 0f64);
    for (name, expected) in &golden.images {
        let lab = rgb_to_lab(&detection_rgb(name));
        let fg = foreground(&lab, &background_colors(&lab));
        let minimum_area = 0.02 * (fg.width * fg.height) as f64;

        let mut ours = Vec::new();
        for rect in &expected.deduplicated {
            ours.extend(split_on_gutter(&rect.rect(), &fg, minimum_area));
        }
        match match_rects(&ours, &expected.split, 0.05) {
            Some(gap) => {
                exact += 1;
                worst = worst.max(gap);
            }
            None => println!(
                "  {name}: split differs ({} vs python {})",
                ours.len(),
                expected.split.len()
            ),
        }
    }
    let n = golden.images.len();
    println!("{n} images: split exact {exact}, worst gap {worst:.4}");
    // Misses can only come from the few images whose foreground differs.
    assert!(exact >= n - 3, "split exact on only {exact}");
}

#[test]
fn angle_snap_matches_python() {
    if !reference_available() {
        return;
    }
    let golden = golden();
    let (mut rects, mut exact, mut worst, mut changed) = (0, 0, 0f64, 0);
    let mut problems = Vec::new();
    for (name, expected) in &golden.images {
        let gray = rgb_to_gray(&detection_rgb(name));
        let (w, h) = (gray.width() as usize, gray.height() as usize);
        for (index, (input, want)) in expected.split.iter().zip(&expected.snapped).enumerate() {
            let ours = snap_angle_to_long_edges(gray.as_raw(), w, h, &input.rect());
            let gap = (ours.angle - want.angle).abs();
            rects += 1;
            changed += ((want.angle - input.angle).abs() > 1e-9) as usize;
            if gap < 0.01 {
                exact += 1;
            } else {
                problems.push(format!(
                    "{name}#{index}: angle {:.4} (input {:.4}) vs python {:.4}",
                    ours.angle, input.angle, want.angle
                ));
            }
            worst = worst.max(gap);
        }
    }
    println!(
        "{rects} rectangles ({changed} snapped by Python): angle exact {exact}, worst angle gap {worst:.4} deg"
    );
    for problem in problems.iter().take(25) {
        println!("  {problem}");
    }
    assert_eq!(exact, rects, "angle snap differs from Python");
}

#[test]
fn edge_refinement_matches_python() {
    if !reference_available() {
        return;
    }
    let golden = golden();
    let (mut rects, mut exact, mut moved, mut worst) = (0, 0, 0, 0f64);
    let mut problems = Vec::new();
    for (name, expected) in &golden.images {
        let gray = rgb_to_gray(&detection_rgb(name));
        let (w, h) = (gray.width() as usize, gray.height() as usize);
        for (index, (input, want)) in expected.snapped.iter().zip(&expected.refined).enumerate() {
            let ours = refine_rect_to_edges(gray.as_raw(), w, h, &input.rect(), 0.12);
            let gap = rect_gap(ours, want.rect());
            rects += 1;
            moved += (rect_gap(input.rect(), want.rect()) > 1e-6) as usize;
            worst = worst.max(gap);
            if gap < 0.05 {
                exact += 1;
            } else {
                problems.push(format!(
                    "{name}#{index}: ours ({:.2},{:.2}) {:.2}x{:.2} vs python ({:.2},{:.2}) {:.2}x{:.2}",
                    ours.cx, ours.cy, ours.w, ours.h, want.cx, want.cy, want.w, want.h
                ));
            }
        }
    }
    println!("{rects} rectangles ({moved} moved by Python): exact {exact}, worst gap {worst:.4}");
    for problem in problems.iter().take(20) {
        println!("  {problem}");
    }
    assert_eq!(exact, rects, "edge refinement differs from Python");
}

/// Full pipeline on one image, compared with Python's final regions.
/// Returns `None` when the region counts differ, else the worst
/// `(position/size gap in px, angle gap in degrees)`.
fn compare_regions(source: &image::RgbImage, expected: &GoldenImage) -> Option<(f64, f64)> {
    let options = DetectOptions {
        inset: 0,
        padding: 0,
        ..Default::default()
    };
    let detection = detect_v3(source, &options);
    let ours: Vec<RotatedRect> = detection
        .regions
        .iter()
        .map(|r| r.rect.canonical())
        .collect();
    if ours.len() != expected.regions.len() {
        return None;
    }
    let mut unused = ours;
    let (mut worst_px, mut worst_deg) = (0f64, 0f64);
    for want in &expected.regions {
        let want = want.canonical();
        let (index, _) = unused
            .iter()
            .enumerate()
            .map(|(i, r)| (i, (r.cx - want.cx).abs() + (r.cy - want.cy).abs()))
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        let got = unused.swap_remove(index);
        let period = if (got.w - got.h).abs() < 1.0 {
            90.0
        } else {
            180.0
        };
        let mut degrees = (got.angle - want.angle).abs() % period;
        degrees = degrees.min(period - degrees);
        worst_px = worst_px
            .max((got.cx - want.cx).abs())
            .max((got.cy - want.cy).abs())
            .max((got.w - want.w).abs())
            .max((got.h - want.h).abs());
        worst_deg = worst_deg.max(degrees);
    }
    Some((worst_px, worst_deg))
}

#[test]
fn full_pipeline_matches_python_on_identical_pixels() {
    if !reference_available() {
        return;
    }
    let golden = golden();
    let (mut matched, mut within_tolerance, mut worst_px, mut worst_deg) = (0, 0, 0f64, 0f64);
    for (name, expected) in &golden.images {
        let decoded = load_image(
            &workspace_root()
                .join("reference/decoded")
                .join(format!("{name}.png")),
        )
        .unwrap();
        match compare_regions(&to_detection_rgb(&decoded), expected) {
            Some((px, deg)) => {
                matched += 1;
                within_tolerance += (px <= 2.0 && deg <= 0.5) as usize;
                worst_px = worst_px.max(px);
                worst_deg = worst_deg.max(deg);
                if px > 2.0 || deg > 0.5 {
                    println!("  {name}: {px:.3} px, {deg:.3} deg");
                }
            }
            None => println!("  {name}: region count differs from Python"),
        }
    }
    let n = golden.images.len();
    println!(
        "{n} images: same region count {matched}, within 2 px / 0.5 deg {within_tolerance}, worst {worst_px:.3} px {worst_deg:.3} deg"
    );
    // Phase 1 target: within 2 px / 0.5 deg of Python. Known exceptions: two images
    // whose foreground differs from Python by a few pixels (low-contrast-02, tight-gutters-02)
    // and one where float32 rounding in minAreaRect tips a floor() (scansplitter-08).
    assert_eq!(matched, n, "region counts differ from Python");
    assert!(
        within_tolerance >= n - 3,
        "only {within_tolerance} of {n} within tolerance"
    );
}

#[test]
fn full_pipeline_on_the_original_jpegs() {
    if !reference_available() {
        return;
    }
    let golden = golden();
    let (mut matched, mut within_tolerance, mut worst_px, mut worst_deg, mut jpegs) =
        (0, 0, 0f64, 0f64, 0);
    for (name, expected) in &golden.images {
        if !name.ends_with(".jpg") {
            continue;
        }
        jpegs += 1;
        let image =
            load_image(&workspace_root().join("reference/ScanSplitter").join(name)).unwrap();
        match compare_regions(&to_detection_rgb(&image), expected) {
            Some((px, deg)) => {
                matched += 1;
                within_tolerance += (px <= 2.0 && deg <= 0.5) as usize;
                worst_px = worst_px.max(px);
                worst_deg = worst_deg.max(deg);
                if px > 2.0 || deg > 0.5 {
                    println!("  {name}: {px:.3} px, {deg:.3} deg");
                }
            }
            None => println!("  {name}: region count differs from Python"),
        }
    }
    println!(
        "{jpegs} JPEGs: same region count {matched}, within 2 px / 0.5 deg {within_tolerance}, worst {worst_px:.3} px {worst_deg:.3} deg"
    );
}

#[test]
fn first_divergent_stage_per_image() {
    if !reference_available() {
        return;
    }
    let golden = golden();
    let mut diverged = 0;
    for (name, expected) in &golden.images {
        let decoded = load_image(
            &workspace_root()
                .join("reference/decoded")
                .join(format!("{name}.png")),
        )
        .unwrap();
        let options = DetectOptions {
            inset: 0,
            padding: 0,
            ..Default::default()
        };
        let stages = detect_v3(&to_detection_rgb(&decoded), &options).stages;
        let checks: [(&str, &[RotatedRect], &[GoldenRect]); 6] = [
            ("candidates", &stages.candidates, &expected.candidates),
            ("merged", &stages.merged, &expected.merged),
            ("deduplicated", &stages.deduplicated, &expected.deduplicated),
            ("split", &stages.split, &expected.split),
            ("snapped", &stages.snapped, &expected.snapped),
            ("refined", &stages.refined, &expected.refined),
        ];
        let first = checks
            .iter()
            .find(|(_, ours, theirs)| match_rects(ours, theirs, 0.05).is_none());
        if let Some((stage, ours, theirs)) = first {
            diverged += 1;
            let fg_pixels = stages
                .foreground_mask
                .as_raw()
                .iter()
                .filter(|&&v| v != 0)
                .count();
            println!(
                "  {name}: first differs at {stage} ({} vs python {}); foreground {fg_pixels} vs {}",
                ours.len(),
                theirs.len(),
                expected.foreground_count
            );
        }
    }
    println!(
        "{} images, {diverged} diverge at some stage",
        golden.images.len()
    );
}
