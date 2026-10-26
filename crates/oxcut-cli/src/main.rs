//! `oxcut`: detect and crop prints from flatbed scans.

mod boxes;
mod output;
mod overlay;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use oxcut_core::crop::crop_region;
use oxcut_core::detect::{DetectOptions, detect_v3};
use oxcut_core::io::{load_image, to_detection_rgb};

use boxes::{BoxRegion, BoxesFile};
use output::Format;

#[derive(Parser)]
#[command(
    name = "oxcut",
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
}

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    /// Model-free, background-aware detector (a port of ScanSplitter v3).
    V3,
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
    /// Pixels trimmed from every side of each box (removes the soft print edge).
    #[arg(long, default_value_t = 10)]
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
struct CropArgs {
    /// The scan the boxes were detected in.
    input: PathBuf,
    /// Boxes file written by `oxcut detect`.
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
    }
}

fn detect(args: DetectArgs) -> Result<()> {
    if !(0.0..=100.0).contains(&args.min_area) || !(0.0..=100.0).contains(&args.max_area) {
        bail!("--min-area and --max-area are percentages between 0 and 100");
    }
    let Mode::V3 = args.mode;

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
    let detection = detect_v3(&source, &options);

    let file = BoxesFile {
        version: 1,
        source: args
            .input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        width: source.width(),
        height: source.height(),
        mode: "v3".into(),
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
            "{} is not a boxes file from `oxcut detect`",
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
    std::fs::create_dir_all(&args.output)?;

    let stem = args
        .input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "scan".into());
    let format = args.format.resolve(&args.input);
    let mut written = 0;
    for (index, region) in file.regions.iter().enumerate() {
        let rect =
            oxcut_core::RotatedRect::new(region.cx, region.cy, region.w, region.h, region.angle);
        let Some(crop) = crop_region(&image, &rect) else {
            eprintln!("skipping box {}: empty after clipping", index + 1);
            continue;
        };
        let path = args
            .output
            .join(format!("{stem}-{}.{}", index + 1, format.extension()));
        refuse_to_overwrite_source(&path, &args.input)?;
        output::save(&crop, &path, format, args.quality)?;
        eprintln!(
            "wrote {} ({}x{})",
            path.display(),
            crop.width(),
            crop.height()
        );
        written += 1;
    }
    eprintln!(
        "{written} image{} written to {}",
        if written == 1 { "" } else { "s" },
        args.output.display()
    );
    Ok(())
}

/// The scan itself is never modified.
fn refuse_to_overwrite_source(target: &Path, source: &Path) -> Result<()> {
    if let (Ok(a), Ok(b)) = (target.canonicalize(), source.canonicalize())
        && a == b
    {
        bail!("refusing to overwrite the source scan {}", source.display());
    }
    Ok(())
}
