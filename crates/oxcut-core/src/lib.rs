//! Oxcut engine: detection, deskew, crop and export of photos on flatbed scans.
//!
//! Detection is a port of ScanSplitter (GPL-3.0). See `docs/detector-v7-port.md`.

pub mod geometry;
pub mod io;
pub mod resize;

pub use geometry::RotatedRect;
