//! OxiCut engine: detection, deskew, crop and export of photos on flatbed scans.
//!
//! Detection is a port of ScanSplitter (GPL-3.0). See `docs/detector-v7-port.md`.

pub mod background;
pub mod candidates;
pub mod canny;
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
pub mod io;
pub mod kmeans;
pub mod mask;
pub mod merge;
pub mod refine;
pub mod resize;
pub mod snap;
pub mod split;
pub mod warp;

pub use geometry::RotatedRect;
