//! `oxicut`: detect and crop prints from flatbed scans.

mod bench;
mod boxes;
mod overlay;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use oxicut_core::RotatedRect;
use oxicut_core::detect::{DetectOptions, Detection, detect_v3};
use oxicut_core::export::{Collision, ExportSettings, OutputFormat, export_regions};
use oxicut_core::io::{load_image, to_detection_rgb};
use oxicut_core::v7::detect_v7_lite;

use boxes::{BoxRegion, BoxesFile};

#[derive(Parser)]
#[command(
    name = "oxicut",
    version,
    about = "Split flatbed scans into separate, straightened prints"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Find the prints in a scan and write their boxes as JSON.
    Detect(DetectArgs),
    /// Cut the boxes from a `detect` run out of the scan as separate images.
    Crop(CropArgs),
    /// Score a detector against ground-truth boxes (ScanSplitter's metrics).
    Bench(BenchCli),
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Mode {
    /// Model-free, background-aware detector (a port of ScanSplitter v3).
    V3,
    /// v3 plus v7's inner-edge tightening: boxes move off white borders and shadows.
    V7Lite,
}

impl Mode {
    /// Name written to `boxes.json`.
    fn name(self) -> &'static str {
        match self {
            Mode::V3 => "v3",
            Mode::V7Lite => "v7-lite",
        }
    }
}

#[derive(Args)]
struct DetectArgs {
    /// Scan to read (JPEG, PNG or TIFF, 8 or 16 bit).
    input: PathBuf,
    #[arg(long, value_enum, default_value = "v3")]
    mode: Mode,
    /// Smallest accepted print, as a percentage of the scan's area.
    #[arg(long, default_value_t = 2.0)]
    min_area: f64,
    /// Largest accepted print, as a percentage of the scan's area.
    #[arg(long, default_value_t = 80.0)]
    max_area: f64,
    /// Pixels trimmed from every side of each box. 0 matches the printed photo's
    /// edge best; ScanSplitter's own default of 10 shrinks every box.
    #[arg(long, default_value_t = 0)]
    inset: i32,
    /// Pixels added around every box.
    #[arg(long, default_value_t = 0)]
    padding: i32,
    /// Write the boxes here instead of standard output.
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Also write `boxes.json`, `debug.png` (final boxes), `stages.png` (every stage)
    /// and the foreground and region masks into this directory.
    #[arg(long)]
    debug: Option<PathBuf>,
}

#[derive(Args)]
struct BenchCli {
    /// Benchmark manifest. Images are found relative to its folder.
    #[arg(
        long,
        default_value = "reference/ScanSplitter/benchmarks/manifest.json"
    )]
    manifest: PathBuf,
    /// Suite to run (repeat for several); default: every suite except `album`.
    #[arg(long = "suite")]
    suites: Vec<String>,
    #[arg(long, value_enum, default_value = "v3")]
    mode: Mode,
    /// Pixels trimmed from every side. ScanSplitter's own benchmark uses its default, 10.
    #[arg(long, default_value_t = 0)]
    inset: i32,
    /// Print one line per scan as well.
    #[arg(long)]
    per_case: bool,
    /// Scans processed at the same time.
    #[arg(long, default_value_t = 4)]
    jobs: usize,
}

#[derive(Args)]
struct CropArgs {
    /// The scan the boxes were detected in.
    input: PathBuf,
    /// Boxes file written by `oxicut detect`.
    #[arg(long)]
    boxes: PathBuf,
    /// Directory for the cropped images (created if missing).
    #[arg(short, long)]
    output: PathBuf,
    /// Output format; `same` keeps the scan's own format.
    #[arg(long, value_enum, default_value = "same")]
    format: Format,
    /// JPEG quality, 1-100.
    #[arg(long, default_value_t = 92)]
    quality: u8,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Detect(args) => detect(args),
        Command::Crop(args) => crop(args),
        Command::Bench(args) => bench::run(bench::BenchArgs {
            manifest: args.manifest,
            suites: args.suites,
            mode: args.mode,
            options: DetectOptions {
                inset: args.inset,
                ..Default::default()
            },
            per_case: args.per_case,
            jobs: args.jobs,
        }),
    }
}

/// Run `mode` on a scan (the one place detectors are chosen).
fn run_detection(mode: Mode, rgb: &image::RgbImage, options: &DetectOptions) -> Detection {
    match mode {
        Mode::V3 => detect_v3(rgb, options),
        Mode::V7Lite => detect_v7_lite(rgb, options),
    }
}

/// Just the boxes.
fn run_detector(mode: Mode, rgb: &image::RgbImage, options: &DetectOptions) -> Vec<RotatedRect> {
    run_detection(mode, rgb, options)
        .regions
        .into_iter()
        .map(|r| r.rect)
        .collect()
}
fn detect(args: DetectArgs) -> Result<()> {
    if !(0.0..=100.0).contains(&args.min_area) || !(0.0..=100.0).contains(&args.max_area) {
        bail!("--min-area and --max-area are percentages between 0 and 100");
    }
    let image =
        load_image(&args.input).with_context(|| format!("reading {}", args.input.display()))?;
    let source = to_detection_rgb(&image);
    let options = DetectOptions {
        min_area_ratio: args.min_area / 100.0,
        max_area_ratio: args.max_area / 100.0,
        padding: args.padding,
        inset: args.inset,
        ..Default::default()
    };
    let detection = run_detection(args.mode, &source, &options);

    let file = BoxesFile {
        version: 1,
        source: args
            .input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        width: source.width(),
        height: source.height(),
        mode: args.mode.name().into(),
        regions: detection
            .regions
            .iter()
            .map(|r| BoxRegion {
                cx: r.rect.cx,
                cy: r.rect.cy,
                w: r.rect.w,
                h: r.rect.h,
                angle: r.rect.angle,
                area_ratio: r.area_ratio,
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&file)?;

    if let Some(path) = &args.output {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(path, &json).with_context(|| format!("writing {}", path.display()))?;
    } else if args.debug.is_none() {
        println!("{json}");
    }

    if let Some(dir) = &args.debug {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("boxes.json"), &json)?;
        overlay::write_debug_images(dir, &source, &detection)?;
        eprintln!("debug output in {}", dir.display());
    }
    eprintln!(
        "{}: {} print{} found ({}x{} px)",
        args.input.display(),
        file.regions.len(),
        if file.regions.len() == 1 { "" } else { "s" },
        file.width,
        file.height
    );
    Ok(())
}

fn crop(args: CropArgs) -> Result<()> {
    if !(1..=100).contains(&args.quality) {
        bail!("--quality must be between 1 and 100");
    }
    let file: BoxesFile = serde_json::from_str(
        &std::fs::read_to_string(&args.boxes)
            .with_context(|| format!("reading {}", args.boxes.display()))?,
    )
    .with_context(|| {
        format!(
            "{} is not a boxes file from `oxicut detect`",
            args.boxes.display()
        )
    })?;

    let image =
        load_image(&args.input).with_context(|| format!("reading {}", args.input.display()))?;
    if (image.width(), image.height()) != (file.width, file.height) {
        bail!(
            "{} is {}x{} but the boxes were detected in a {}x{} scan",
            args.input.display(),
            image.width(),
            image.height(),
            file.width,
            file.height
        );
    }
    let regions: Vec<RotatedRect> = file
        .regions
        .iter()
        .map(|r| RotatedRect::new(r.cx, r.cy, r.w, r.h, r.angle))
        .collect();
    let settings = ExportSettings {
        format: args.format.into(),
        jpeg_quality: args.quality,
        pattern: "{source}-{n}".into(),
        // Re-running a command line export refreshes its files.
        collision: Collision::Overwrite,
    };
    let written = export_regions(&image, &args.input, &regions, &args.output, &settings)?;
    for file in &written {
        eprintln!(
            "wrote {} ({}x{})",
            file.path.display(),
            file.width,
            file.height
        );
    }
    if written.len() < regions.len() {
        eprintln!(
            "{} box{} fell outside the scan and was skipped",
            regions.len() - written.len(),
            if regions.len() - written.len() == 1 {
                ""
            } else {
                "es"
            }
        );
    }
    eprintln!(
        "{} image{} written to {}",
        written.len(),
        if written.len() == 1 { "" } else { "s" },
        args.output.display()
    );
    Ok(())
}

/// Output format as typed on the command line.
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    /// Keep the scan's own format.
    Same,
    Jpg,
    Png,
    Tiff,
}

impl From<Format> for OutputFormat {
    fn from(format: Format) -> Self {
        match format {
            Format::Same => OutputFormat::Same,
            Format::Jpg => OutputFormat::Jpg,
            Format::Png => OutputFormat::Png,
            Format::Tiff => OutputFormat::Tiff,
        }
    }
}
