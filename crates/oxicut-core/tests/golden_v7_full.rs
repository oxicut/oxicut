//! Full v7 against ScanSplitter, stage by stage (`benchmarks/golden/v7_full.json`, made by
//! `tools/dump_v7_full_reference.py`). Needs the reference checkout and the MobileSAM
//! models; skips without them. Each stage's rectangles are matched by IoU, and the summary
//! is printed (run with `--nocapture`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use oxicut_core::detect::DetectOptions;
use oxicut_core::geometry::{RotatedRect, iou};
use oxicut_core::io::{load_image, to_detection_rgb};
use oxicut_core::sam::MobileSam;
use oxicut_core::score::Candidate;
use oxicut_core::v5;
use oxicut_core::v7::detect_v7;
use serde::Deserialize;

#[derive(Deserialize, Clone, Copy)]
struct Rect {
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    angle: f64,
}

impl From<Rect> for RotatedRect {
    fn from(r: Rect) -> Self {
        RotatedRect::new(r.cx, r.cy, r.w, r.h, r.angle)
    }
}

#[derive(Deserialize)]
struct Item {
    rect: Rect,
    score: f64,
    sharp: f64,
    ghost: [f64; 2],
}

#[derive(Deserialize)]
struct Texture {
    rect: Rect,
    touches: bool,
}

#[derive(Deserialize)]
struct Image {
    v3: Vec<Rect>,
    v3c: Vec<Rect>,
    chroma: Vec<Rect>,
    texture: Vec<Texture>,
    frames: Vec<Rect>,
    v5: Vec<Rect>,
    anchors: Vec<Item>,
    alternatives: Vec<Item>,
    chosen: Vec<Item>,
    #[serde(rename = "final")]
    final_items: Vec<Item>,
}

#[derive(Deserialize)]
struct Golden {
    images: BTreeMap<String, Image>,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn models() -> Option<(PathBuf, PathBuf)> {
    let dir = std::env::var_os("OXICUT_MODELS_DIR")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(std::env::var_os("APPDATA")?).join("oxicut/models")))?;
    let (e, d) = (
        dir.join("mobile_sam_image_encoder.onnx"),
        dir.join("sam_mask_decoder_single.onnx"),
    );
    (e.is_file() && d.is_file()).then_some((e, d))
}

/// How many of `want` have a counterpart in `got` (IoU above 0.95), and the reverse.
fn matched(got: &[RotatedRect], want: &[RotatedRect]) -> (usize, usize) {
    let hit = |a: &RotatedRect, set: &[RotatedRect]| set.iter().any(|b| iou(a, b) > 0.95);
    (
        want.iter().filter(|w| hit(w, got)).count(),
        got.iter().filter(|g| hit(g, want)).count(),
    )
}

#[derive(Default)]
struct Tally {
    want: usize,
    got: usize,
    want_found: usize,
    got_found: usize,
    images_exact: usize,
    images: usize,
}

impl Tally {
    fn add(&mut self, name: &str, stage: &str, got: &[RotatedRect], want: &[RotatedRect]) {
        let (want_found, got_found) = matched(got, want);
        self.want += want.len();
        self.got += got.len();
        self.want_found += want_found;
        self.got_found += got_found;
        self.images += 1;
        if want_found == want.len() && got_found == got.len() {
            self.images_exact += 1;
        } else {
            eprintln!(
                "  {stage} differs on {name}: python {} rust {} (python found {want_found}, rust found {got_found})",
                want.len(),
                got.len()
            );
        }
    }

    fn report(&self, stage: &str) {
        eprintln!(
            "{stage:<14} images identical {:>2}/{:<2}  rects python {:>3} rust {:>3}  python matched {:>3} rust matched {:>3}",
            self.images_exact, self.images, self.want, self.got, self.want_found, self.got_found
        );
    }
}

fn rects(items: &[Item]) -> Vec<RotatedRect> {
    items.iter().map(|i| i.rect.into()).collect()
}

fn candidates(items: &[Candidate]) -> Vec<RotatedRect> {
    items.iter().map(|c| c.rect).collect()
}

#[test]
fn v7_matches_scansplitter_stage_by_stage() {
    let root = root();
    if !root.join("reference/ScanSplitter/benchmarks").is_dir() {
        eprintln!("skipping: reference/ScanSplitter is not checked out");
        return;
    }
    let Some((encoder, decoder)) = models() else {
        eprintln!("skipping: MobileSAM models are not downloaded");
        return;
    };
    let golden: Golden = serde_json::from_str(
        &std::fs::read_to_string(root.join("benchmarks/golden/v7_full.json")).unwrap(),
    )
    .unwrap();
    let mut sam = MobileSam::load(&encoder, &decoder).unwrap();
    let options = DetectOptions {
        inset: 0,
        ..Default::default()
    };
    // The full set of 114 takes several minutes; V7_FIXTURES=all runs it.
    let limit: usize = match std::env::var("V7_FIXTURES").as_deref() {
        Ok("all") => usize::MAX,
        Ok(n) => n.parse().unwrap_or(12),
        Err(_) => 12,
    };

    let names = [
        "v3",
        "v3c",
        "chroma",
        "texture",
        "frames",
        "v5",
        "anchors",
        "alternatives",
        "chosen",
        "final",
    ];
    let mut tallies: Vec<Tally> = names.iter().map(|_| Tally::default()).collect();
    let (mut score_error, mut sharp_error, mut ghost_error) = (0f64, 0f64, 0f64);
    let mut compared = 0usize;

    for (name, want) in golden.images.iter().take(limit) {
        let decoded =
            load_image(&root.join("reference/decoded").join(format!("{name}.png"))).unwrap();
        let rgb = to_detection_rgb(&decoded);
        let (_, stages) = detect_v7(&rgb, &options, &mut sam).unwrap();
        let textures = v5::texture_rectangles(&rgb, options.min_area_ratio, options.max_area_ratio);
        let frames = v5::frame_rectangles(&rgb, options.min_area_ratio, options.max_area_ratio);

        let to = |v: &[Rect]| -> Vec<RotatedRect> { v.iter().map(|&r| r.into()).collect() };
        let texture_rects: Vec<RotatedRect> = textures.iter().map(|t| t.0).collect();
        let want_textures: Vec<RotatedRect> = want.texture.iter().map(|t| t.rect.into()).collect();
        if textures.iter().map(|t| t.1).collect::<Vec<_>>()
            != want.texture.iter().map(|t| t.touches).collect::<Vec<_>>()
        {
            eprintln!("  texture edge flags differ on {name}");
        }

        tallies[0].add(name, "v3", &stages.v3, &to(&want.v3));
        tallies[1].add(name, "v3c", &stages.v3_capped, &to(&want.v3c));
        tallies[2].add(name, "chroma", &stages.chroma, &to(&want.chroma));
        tallies[3].add(name, "texture", &texture_rects, &want_textures);
        tallies[4].add(name, "frames", &frames, &to(&want.frames));
        tallies[5].add(name, "v5", &stages.v5, &to(&want.v5));
        tallies[6].add(
            name,
            "anchors",
            &candidates(&stages.anchors),
            &rects(&want.anchors),
        );
        tallies[7].add(
            name,
            "alternatives",
            &candidates(&stages.alternatives),
            &rects(&want.alternatives),
        );
        tallies[8].add(
            name,
            "chosen",
            &candidates(&stages.chosen),
            &rects(&want.chosen),
        );
        tallies[9].add(
            name,
            "final",
            &candidates(&stages.final_candidates),
            &rects(&want.final_items),
        );

        // Evidence values, for candidates that line up one to one.
        for (got, expected) in stages.alternatives.iter().zip(&want.alternatives) {
            let r: RotatedRect = expected.rect.into();
            if iou(&got.rect, &r) > 0.99 {
                compared += 1;
                score_error = score_error.max((got.score - expected.score).abs());
                sharp_error = sharp_error.max((got.sharp - expected.sharp).abs());
                ghost_error = ghost_error
                    .max((got.low_std - expected.ghost[0]).abs())
                    .max((got.fine - expected.ghost[1]).abs());
            }
        }
    }
    for (name, tally) in names.iter().zip(&tallies) {
        tally.report(name);
    }
    eprintln!(
        "evidence over {compared} aligned candidates: max |score| {score_error:.4}, |sharp| {sharp_error:.5}, |ghost| {ghost_error:.4}"
    );
}
