//! `oxicut bench`: score a detector against ground-truth boxes, with the same
//! metrics (and manifest format) as ScanSplitter's `benchmarks/evaluate.py`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use oxicut_core::RotatedRect;
use oxicut_core::bench::{CaseScore, Summary, score_rectangles, summarize};
use oxicut_core::detect::DetectOptions;
use oxicut_core::io::{load_image, to_detection_rgb};
use serde::Deserialize;

use crate::Mode;

#[derive(Deserialize)]
struct Manifest {
    cases: Vec<Case>,
}

#[derive(Deserialize, Clone)]
struct Case {
    id: String,
    suite: String,
    /// Relative to the manifest's folder.
    image: String,
    /// `[centre x, centre y, width, height, angle in degrees]` per ground-truth box.
    rectangles: Vec<[f64; 5]>,
}

pub struct BenchArgs {
    pub manifest: PathBuf,
    /// Suites to run; empty means every suite except `album` (a different detector).
    pub suites: Vec<String>,
    pub mode: Mode,
    pub options: DetectOptions,
    pub per_case: bool,
    pub jobs: usize,
}

fn run_case(root: &Path, case: &Case, args: &BenchArgs) -> Result<CaseScore> {
    let path = root.join(&case.image);
    let image = load_image(&path).with_context(|| format!("reading {}", path.display()))?;
    let rgb = to_detection_rgb(&image);
    drop(image);

    let started = Instant::now();
    let found = crate::run_detector(args.mode, &rgb, &args.options)?;
    let seconds = started.elapsed().as_secs_f64();

    let expected: Vec<RotatedRect> = case
        .rectangles
        .iter()
        .map(|r| RotatedRect::new(r[0], r[1], r[2], r[3], r[4]))
        .collect();
    let mut score = score_rectangles(&expected, &found);
    score.runtime_seconds = seconds;
    Ok(score)
}

pub fn run(args: BenchArgs) -> Result<()> {
    let text = std::fs::read_to_string(&args.manifest)
        .with_context(|| format!("reading {}", args.manifest.display()))?;
    let manifest: Manifest = serde_json::from_str(&text)
        .with_context(|| format!("{} is not a benchmark manifest", args.manifest.display()))?;
    let root = args
        .manifest
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();

    let cases: Vec<Case> = manifest
        .cases
        .into_iter()
        .filter(|c| {
            if args.suites.is_empty() {
                c.suite != "album"
            } else {
                args.suites.contains(&c.suite)
            }
        })
        .collect();
    if cases.is_empty() {
        bail!("no cases match; suites in the manifest are chosen with --suite");
    }

    // A few scans at a time; each decode is large.
    let results: Mutex<Vec<Option<Result<CaseScore>>>> =
        Mutex::new((0..cases.len()).map(|_| None).collect());
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..args.jobs.clamp(1, cases.len()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(case) = cases.get(index) else { break };
                    let outcome = run_case(&root, case, &args);
                    results.lock().expect("results lock")[index] = Some(outcome);
                }
            });
        }
    });

    let mut scored: Vec<(&Case, CaseScore)> = Vec::new();
    for (case, outcome) in cases
        .iter()
        .zip(results.into_inner().expect("results lock"))
    {
        scored.push((
            case,
            outcome
                .expect("every case ran")
                .with_context(|| format!("case {}", case.id))?,
        ));
    }

    let mut suites: Vec<&str> = scored.iter().map(|(c, _)| c.suite.as_str()).collect();
    suites.dedup();
    suites.sort_unstable();
    suites.dedup();

    println!(
        "Detector `{:?}`, trim {} px, padding {} px\n",
        args.mode, args.options.inset, args.options.padding
    );
    println!(
        "| Suite | Cases | Box quality | Tightness | Coverage | F1@0.50 | F1@0.85 | Count accuracy | Time |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    let row = |name: &str, s: &Summary| {
        println!(
            "| {name} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}s |",
            s.cases,
            s.box_quality * 100.0,
            s.crop_tightness * 100.0,
            s.content_coverage * 100.0,
            s.f1 * 100.0,
            s.strict_f1 * 100.0,
            s.count_accuracy * 100.0,
            s.runtime_seconds
        );
    };
    for suite in &suites {
        let scores: Vec<CaseScore> = scored
            .iter()
            .filter(|(c, _)| c.suite == *suite)
            .map(|(_, s)| s.clone())
            .collect();
        row(suite, &summarize(&scores));
    }
    if suites.len() > 1 {
        let all: Vec<CaseScore> = scored.iter().map(|(_, s)| s.clone()).collect();
        row("all", &summarize(&all));
    }

    if args.per_case {
        println!(
            "\n| Case | Expected | Found | Box quality | Tightness | Coverage | F1@0.50 | F1@0.85 |"
        );
        println!("|---|---:|---:|---:|---:|---:|---:|---:|");
        for (case, s) in &scored {
            println!(
                "| {} | {} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% |",
                case.id,
                s.expected,
                s.detected,
                s.box_quality() * 100.0,
                s.tightness() * 100.0,
                s.coverage() * 100.0,
                s.f1_at_detection() * 100.0,
                s.f1_at_strict() * 100.0
            );
        }
    }
    Ok(())
}
