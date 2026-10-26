//! The `boxes.json` file `detect` writes and `crop` reads.

use serde::{Deserialize, Serialize};

/// One print as a rotated rectangle in source pixels: centre, size (`w >= h`)
/// and the angle in degrees of the `w` axis, clockwise with y pointing down.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxRegion {
    pub cx: f64,
    pub cy: f64,
    pub w: f64,
    pub h: f64,
    pub angle: f64,
    /// Share of the scan's area covered by the box before inset/padding.
    #[serde(default)]
    pub area_ratio: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BoxesFile {
    pub version: u32,
    /// File name of the scan the boxes belong to.
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub mode: String,
    pub regions: Vec<BoxRegion>,
}
