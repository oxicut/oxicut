//! OxiCut engine: detection, deskew, crop and export of photos on flatbed scans.
//!
//! Detection is a port of ScanSplitter (GPL-3.0). See `docs/detector-v7-port.md`.

/// The OxiCut version shown in the app and the CLI and written into the `Software` field of
/// exported images. Hardcoded for now; it will come from the build later.
pub const VERSION: &str = "0.0.1";

pub mod background;
pub mod bench;
pub mod candidates;
pub mod canny;
pub mod chroma;
pub mod color;
pub mod contours;
pub mod crop;
pub mod detect;
pub mod export;
pub mod filter;
pub mod flags;
pub mod foreground;
pub mod geometry;
pub mod hough;
pub mod imgproc;
pub mod io;
pub mod kmeans;
pub mod mask;
pub mod merge;
pub mod metadata;
pub mod pil;
pub mod refine;
pub mod resize;
pub mod sam;
pub mod score;
pub mod snap;
pub mod split;
pub mod tighten;
pub mod v5;
pub mod v7;
pub mod warp;

pub use geometry::RotatedRect;
