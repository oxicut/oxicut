//! Oxcut engine: detection, deskew, crop and export of photos on flatbed scans.
//!
//! Detection is a port of ScanSplitter (GPL-3.0). See `docs/detector-v7-port.md`.

pub mod background;
pub mod candidates;
pub mod color;
pub mod contours;
pub mod foreground;
pub mod geometry;
pub mod io;
pub mod kmeans;
pub mod mask;
pub mod resize;

pub use geometry::RotatedRect;
