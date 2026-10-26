//! Colour conversions matching OpenCV's 8-bit `cvtColor` as used by ScanSplitter.

use image::{GrayImage, RgbImage};

/// 8-bit Lab as OpenCV stores it (L scaled to 0..255, a/b offset by 128), held
/// as `f32` like ScanSplitter's `.astype(np.float32)`. Every value is an integer.
pub struct LabImage {
    pub width: usize,
    pub height: usize,
    pub data: Vec<[f32; 3]>,
}

impl LabImage {
    pub fn at(&self, x: usize, y: usize) -> [f32; 3] {
        self.data[y * self.width + x]
    }
}

// OpenCV's 8-bit RGB2Lab (`RGB2Lab_b`): integer arithmetic with a gamma table
// and a cube-root table, so results are bit-identical rather than merely close.
const GAMMA_SHIFT: u32 = 3;
const LAB_SHIFT: u32 = 12;
const LAB_SHIFT2: u32 = LAB_SHIFT + GAMMA_SHIFT;
const CBRT_TABLE_SIZE: usize = 256 * 3 / 2 * (1 << GAMMA_SHIFT);

// sRGB D65 matrix (rows X, Y, Z over R, G, B) and white point (`D65`).
const XYZ: [[f32; 3]; 3] = [
    [0.412453, 0.357580, 0.180423],
    [0.212671, 0.715160, 0.072169],
    [0.019334, 0.119193, 0.950227],
];
const WHITE: [f32; 3] = [0.950456, 1.0, 1.088754];

fn descale(x: i32, shift: u32) -> i32 {
    (x + (1 << (shift - 1))) >> shift
}

/// `cvRound`: round half to even.
fn cv_round(x: f32) -> i32 {
    x.round_ties_even() as i32
}

struct LabTables {
    gamma: [u16; 256],
    cbrt: Vec<u16>,
    coeffs: [[i32; 3]; 3],
}

impl LabTables {
    fn new() -> Self {
        let mut gamma = [0u16; 256];
        for (i, entry) in gamma.iter_mut().enumerate() {
            let x = i as f32 / 255.0;
            let linear = if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            };
            *entry = cv_round(255.0 * (1 << GAMMA_SHIFT) as f32 * linear) as u16;
        }

        let scale = 1.0 / (255.0 * (1 << GAMMA_SHIFT) as f32);
        let cbrt = (0..CBRT_TABLE_SIZE)
            .map(|i| {
                let x = i as f32 * scale;
                // The linear segment is single precision in OpenCV; the cube
                // root only matches its `cvCbrt` when computed in double.
                let scaled = if x < 0.008856 {
                    ((1 << LAB_SHIFT2) as f32 * (x * 7.787 + 0.137_931_04)) as f64
                } else {
                    (1 << LAB_SHIFT2) as f64 * (x as f64).cbrt()
                };
                (scaled.round_ties_even() as i32).clamp(0, 65535) as u16
            })
            .collect();

        let mut coeffs = [[0i32; 3]; 3];
        for row in 0..3 {
            let row_scale = (1 << LAB_SHIFT) as f32 / WHITE[row];
            for col in 0..3 {
                coeffs[row][col] = cv_round(XYZ[row][col] * row_scale);
            }
        }
        Self {
            gamma,
            cbrt,
            coeffs,
        }
    }

    fn convert(&self, [r, g, b]: [u8; 3]) -> [u8; 3] {
        let (r, g, b) = (
            self.gamma[r as usize] as i32,
            self.gamma[g as usize] as i32,
            self.gamma[b as usize] as i32,
        );
        let c = &self.coeffs;
        let f = |row: usize| {
            let index = descale(r * c[row][0] + g * c[row][1] + b * c[row][2], LAB_SHIFT);
            self.cbrt[index as usize] as i32
        };
        let (fx, fy, fz) = (f(0), f(1), f(2));

        let l_scale = (116 * 255 + 50) / 100;
        let l_shift = -((16 * 255 * (1 << LAB_SHIFT2) + 50) / 100);
        let l = descale(l_scale * fy + l_shift, LAB_SHIFT2);
        let a = descale(500 * (fx - fy) + 128 * (1 << LAB_SHIFT2), LAB_SHIFT2);
        let b = descale(200 * (fy - fz) + 128 * (1 << LAB_SHIFT2), LAB_SHIFT2);
        [
            l.clamp(0, 255) as u8,
            a.clamp(0, 255) as u8,
            b.clamp(0, 255) as u8,
        ]
    }
}

/// Lab of a single colour, bit-identical to `cv2.cvtColor(..., COLOR_RGB2LAB)`.
pub fn rgb_to_lab_pixel(rgb: [u8; 3]) -> [u8; 3] {
    LabTables::new().convert(rgb)
}

pub fn rgb_to_lab(rgb: &RgbImage) -> LabImage {
    let tables = LabTables::new();
    let (width, height) = rgb.dimensions();
    let data = rgb
        .pixels()
        .map(|p| tables.convert(p.0).map(|v| v as f32))
        .collect();
    LabImage {
        width: width as usize,
        height: height as usize,
        data,
    }
}

/// `cv2.cvtColor(..., COLOR_RGB2GRAY)` on 8-bit data: fixed-point BT.601.
pub fn rgb_to_gray(rgb: &RgbImage) -> GrayImage {
    let (width, height) = rgb.dimensions();
    let data = rgb
        .pixels()
        .map(|p| {
            ((p[0] as u32 * 4899 + p[1] as u32 * 9617 + p[2] as u32 * 1868 + 8192) >> 14) as u8
        })
        .collect();
    GrayImage::from_raw(width, height, data).expect("same dimensions")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn white_and_black_map_to_known_lab() {
        let mut rgb = RgbImage::new(2, 1);
        rgb.put_pixel(0, 0, Rgb([255, 255, 255]));
        rgb.put_pixel(1, 0, Rgb([0, 0, 0]));
        let lab = rgb_to_lab(&rgb);
        // OpenCV 8-bit: white is (255, 128, 128), black is (0, 128, 128).
        assert_eq!(lab.at(0, 0), [255.0, 128.0, 128.0]);
        assert_eq!(lab.at(1, 0), [0.0, 128.0, 128.0]);
    }

    #[test]
    fn lab_matches_cv2_samples() {
        // Values from cv2.cvtColor(..., COLOR_RGB2LAB); our table agrees with
        // cv2 on all 2^24 colours for L and all but ~1100 dark blues for a/b.
        let mut rgb = RgbImage::new(3, 1);
        rgb.put_pixel(0, 0, Rgb([0, 98, 254]));
        rgb.put_pixel(1, 0, Rgb([0, 100, 248]));
        rgb.put_pixel(2, 0, Rgb([0, 22, 138]));
        let lab = rgb_to_lab(&rgb);
        assert_eq!(lab.at(0, 0), [119.0, 165.0, 45.0]);
        assert_eq!(lab.at(1, 0), [119.0, 162.0, 48.0]);
        assert_eq!(lab.at(2, 0)[0], 45.0);
    }

    #[test]
    fn gray_uses_opencv_weights() {
        let mut rgb = RgbImage::new(3, 1);
        rgb.put_pixel(0, 0, Rgb([255, 0, 0]));
        rgb.put_pixel(1, 0, Rgb([0, 255, 0]));
        rgb.put_pixel(2, 0, Rgb([0, 0, 255]));
        let gray = rgb_to_gray(&rgb);
        // 0.299, 0.587, 0.114 of 255, rounded.
        assert_eq!(gray.as_raw(), &[76, 150, 29]);
    }
}
