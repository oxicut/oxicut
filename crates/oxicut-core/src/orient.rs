//! Which way up is a picture? An EfficientNetV2 classifier (ScanSplitter's orientation model) says how far a
//! crop must be turned clockwise to stand upright: 0, 90, 180 or 270 degrees.
//!
//! ScanSplitter takes the model's single answer. On postcards that is wrong too often (paintings and
//! flowers without text), so the crop is shown to the model in all four quarter turns and the four answers
//! are combined: a crop turned a quarter clockwise and answered "turn 90" needs a total of 180. The
//! combined answer carries a confidence, and a crop the model is not sure about is left as it is.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use image::{GrayImage, RgbImage, imageops};
use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;

use crate::geometry::RotatedRect;
use crate::pil::resize_bilinear;

/// Side of the square the model reads.
pub const MODEL_SIDE: usize = 384;
/// The picture is squashed to this square first and then cut to `MODEL_SIDE` in the middle.
const RESIZE_SIDE: u32 = 416;
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

/// A combined answer below this confidence is not acted on. Measured on eight postcards turned four ways:
/// every right answer was at 0.73 or above, the one picture the model cannot tell (a flower without text)
/// was at 0.36.
pub const MIN_CONFIDENCE: f32 = 0.5;

#[derive(Debug, thiserror::Error)]
pub enum OrientError {
    #[error("orientation model: {0}")]
    Runtime(String),
}

fn rt(error: impl std::fmt::Display) -> OrientError {
    OrientError::Runtime(error.to_string())
}

/// What the model made of one crop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orientation {
    /// Quarter turns clockwise that make the crop upright: 0, 1, 2 or 3.
    pub quarter_turns: u8,
    /// Probability of that answer, 0.25 (no idea) to 1.
    pub confidence: f32,
}

impl Orientation {
    /// The quarter turns to apply: the answer when the model is sure enough, else none.
    pub fn decided(&self, min_confidence: f32) -> u8 {
        if self.confidence >= min_confidence {
            self.quarter_turns
        } else {
            0
        }
    }
}

/// `exp` normalised: the model's four scores as probabilities.
pub fn softmax(logits: [f32; 4]) -> [f32; 4] {
    let top = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exp = logits.map(|v| (v - top).exp());
    let sum: f32 = exp.iter().sum();
    exp.map(|v| v / sum)
}

/// Combine the answers for a crop shown in four quarter turns. `per_turn[j]` holds the probabilities for the
/// crop turned `j` quarters clockwise; if that needs `c` more quarters, the crop itself needs `j + c`. The
/// four answers are added as log-probabilities and read back as one distribution.
pub fn combine(per_turn: [[f32; 4]; 4]) -> Orientation {
    let mut total = [0f32; 4];
    for (j, probabilities) in per_turn.iter().enumerate() {
        for (c, p) in probabilities.iter().enumerate() {
            total[(c + j) % 4] += (p + 1e-9).ln();
        }
    }
    let mean = total.map(|v| v / 4.0);
    let probabilities = softmax(mean);
    let (quarter_turns, confidence) = probabilities
        .iter()
        .copied()
        .enumerate()
        .fold((0usize, 0f32), |best, (i, p)| if p > best.1 { (i, p) } else { best });
    Orientation {
        quarter_turns: quarter_turns as u8,
        confidence,
    }
}

/// The model's input for one picture, NCHW and normalised: each colour is squashed to 416 x 416 with Pillow's
/// bilinear filter, the middle 384 x 384 is kept, and the values are scaled like ImageNet's.
pub fn preprocess(image: &RgbImage) -> Vec<f32> {
    let (width, height) = image.dimensions();
    let margin = ((RESIZE_SIDE as usize - MODEL_SIDE) / 2) as u32;
    let mut out = vec![0f32; 3 * MODEL_SIDE * MODEL_SIDE];
    for channel in 0..3 {
        let plane = GrayImage::from_fn(width, height, |x, y| image::Luma([image.get_pixel(x, y).0[channel]]));
        let squashed = resize_bilinear(&plane, RESIZE_SIDE, RESIZE_SIDE);
        let base = channel * MODEL_SIDE * MODEL_SIDE;
        for y in 0..MODEL_SIDE as u32 {
            for x in 0..MODEL_SIDE as u32 {
                let value = squashed.get_pixel(x + margin, y + margin).0[0] as f32 / 255.0;
                out[base + y as usize * MODEL_SIDE + x as usize] = (value - MEAN[channel]) / STD[channel];
            }
        }
    }
    out
}

/// `image` turned `quarters` times clockwise.
pub fn turned(image: &RgbImage, quarters: u8) -> RgbImage {
    match quarters % 4 {
        0 => image.clone(),
        1 => imageops::rotate90(image),
        2 => imageops::rotate180(image),
        _ => imageops::rotate270(image),
    }
}

/// The one ONNX Runtime environment of the process, shared with MobileSAM (see `sam::shared_environment`).
fn environment() -> Result<&'static Environment, OrientError> {
    crate::sam::shared_environment().map_err(OrientError::Runtime)
}

pub struct OrientationModel {
    session: Session,
    input: String,
}

impl OrientationModel {
    pub fn load(path: &Path) -> Result<Self, OrientError> {
        let session = Session::builder(environment()?)
            .map_err(rt)?
            .commit_from_file(path)
            .map_err(rt)?;
        let input = session
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .ok_or_else(|| OrientError::Runtime("the model has no input".into()))?;
        Ok(Self { session, input })
    }

    /// The model's probabilities for each of the pictures, in one run.
    pub fn probabilities(&mut self, images: &[RgbImage]) -> Result<Vec<[f32; 4]>, OrientError> {
        let mut data = Vec::with_capacity(images.len() * 3 * MODEL_SIDE * MODEL_SIDE);
        for image in images {
            data.extend(preprocess(image));
        }
        let shape = vec![images.len() as i64, 3, MODEL_SIDE as i64, MODEL_SIDE as i64];
        let input = Tensor::from_array((shape, data)).map_err(rt)?;
        let outputs = self
            .session
            .run(ort::inputs![self.input.as_str() => input])
            .map_err(rt)?;
        let (_, scores) = outputs[0].try_extract_tensor::<f32>().map_err(rt)?;
        if scores.len() != images.len() * 4 {
            return Err(OrientError::Runtime(format!(
                "expected {} scores, got {}",
                images.len() * 4,
                scores.len()
            )));
        }
        Ok(scores.as_chunks::<4>().0.iter().map(|c| softmax(*c)).collect())
    }

    /// The model's single answer for a picture (ScanSplitter's way): the most likely class and its probability.
    pub fn classify_once(&mut self, image: &RgbImage) -> Result<Orientation, OrientError> {
        let p = self.probabilities(std::slice::from_ref(image))?[0];
        let (quarter_turns, confidence) = p
            .iter()
            .copied()
            .enumerate()
            .fold((0usize, 0f32), |best, (i, v)| if v > best.1 { (i, v) } else { best });
        Ok(Orientation {
            quarter_turns: quarter_turns as u8,
            confidence,
        })
    }

    /// The combined answer over the four quarter turns of the picture.
    pub fn classify(&mut self, image: &RgbImage) -> Result<Orientation, OrientError> {
        let views: Vec<RgbImage> = (0..4).map(|j| turned(image, j)).collect();
        let all = self.probabilities(&views)?;
        Ok(combine([all[0], all[1], all[2], all[3]]))
    }
}

/// The one model of this process. Inference is quick next to detection, so the detect workers take turns
/// instead of each holding an 80 MB session of their own.
static MODEL: Mutex<Option<(PathBuf, OrientationModel)>> = Mutex::new(None);

/// Run `f` with the orientation model from `path`, loading it on first use (and again if the path changes).
pub fn with_orientation<R>(
    path: &Path,
    f: impl FnOnce(&mut OrientationModel) -> R,
) -> Result<R, OrientError> {
    let mut slot = MODEL.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if slot.as_ref().is_none_or(|(loaded, _)| loaded != path) {
        *slot = Some((path.to_path_buf(), OrientationModel::load(path)?));
    }
    let (_, model) = slot.as_mut().expect("loaded above");
    Ok(f(model))
}

/// Keep an angle in [-180, 180), as the screen does for every box.
fn normalise_angle(angle: f64) -> f64 {
    (angle + 180.0).rem_euclid(360.0) - 180.0
}

/// The box with its crop turned `quarter_turns` times clockwise: the way the Right button turns it
/// (sides swapped, angle down a quarter), a half turn leaves the sides and adds 180, and three quarters
/// clockwise is one quarter counter-clockwise (the Left button).
pub fn turn_rect(rect: &RotatedRect, quarter_turns: u8) -> RotatedRect {
    match quarter_turns % 4 {
        0 => *rect,
        1 => RotatedRect::new(rect.cx, rect.cy, rect.h, rect.w, normalise_angle(rect.angle - 90.0)),
        2 => RotatedRect::new(rect.cx, rect.cy, rect.w, rect.h, normalise_angle(rect.angle + 180.0)),
        _ => RotatedRect::new(rect.cx, rect.cy, rect.h, rect.w, normalise_angle(rect.angle + 90.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crop::crop_region;
    use image::{DynamicImage, Rgb};

    /// A picture with a different colour in every corner and a mark near the left edge: no turn looks like another.
    fn asymmetric(width: u32, height: u32) -> RgbImage {
        RgbImage::from_fn(width, height, |x, y| {
            if x < width / 8 && y < height / 2 {
                return Rgb([255, 255, 0]);
            }
            let left = x < width / 2;
            let top = y < height / 2;
            match (left, top) {
                (true, true) => Rgb([200, 30, 30]),
                (false, true) => Rgb([30, 200, 30]),
                (true, false) => Rgb([30, 30, 200]),
                (false, false) => Rgb([220, 220, 220]),
            }
        })
    }

    /// The same picture, give or take the pixels along the colour edges (a crop is sampled on a half-pixel grid, so a turned crop can sit one pixel off): a wrong turn would differ in most pixels.
    fn same_picture(a: &RgbImage, b: &RgbImage) -> bool {
        a.dimensions() == b.dimensions()
            && a.pixels().zip(b.pixels()).filter(|(p, q)| p != q).count() < (a.width() * a.height()) as usize / 25
    }

    #[test]
    fn softmax_adds_up_to_one_and_keeps_the_order() {
        let p = softmax([1.0, 3.0, 0.0, -2.0]);
        assert!((p.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        assert!(p[1] > p[0] && p[0] > p[2] && p[2] > p[3]);
        // Huge scores do not overflow.
        assert!(softmax([1000.0, 0.0, 0.0, 0.0])[0] > 0.999);
    }

    #[test]
    fn four_views_that_agree_give_a_confident_answer() {
        // The crop needs 1 more quarter. Turned j quarters it needs (1 - j) mod 4 more.
        let sure = |needs: usize| {
            let mut p = [0.02f32; 4];
            p[needs % 4] = 0.94;
            p
        };
        let answer = combine([sure(1), sure(0), sure(3), sure(2)]);
        assert_eq!(answer.quarter_turns, 1);
        assert!(answer.confidence > 0.9, "{answer:?}");
        assert_eq!(answer.decided(MIN_CONFIDENCE), 1);
    }

    #[test]
    fn views_that_disagree_leave_the_picture_alone() {
        // Every view insists it needs 180 more: the crop's own answer cannot be told, and it is not acted on.
        let flat = |c: usize| {
            let mut p = [0.1f32; 4];
            p[c] = 0.7;
            p
        };
        let answer = combine([flat(2), flat(2), flat(2), flat(2)]);
        assert!(answer.confidence < MIN_CONFIDENCE, "{answer:?}");
        assert_eq!(answer.decided(MIN_CONFIDENCE), 0);
        // A model that knows nothing is 25% sure of everything.
        let blank = combine([[0.25; 4]; 4]);
        assert!((blank.confidence - 0.25).abs() < 1e-5);
        assert_eq!(blank.decided(MIN_CONFIDENCE), 0);
    }

    #[test]
    fn preprocessing_has_the_models_shape_and_scaling() {
        let grey = RgbImage::from_pixel(50, 80, Rgb([128, 128, 128]));
        let input = preprocess(&grey);
        assert_eq!(input.len(), 3 * MODEL_SIDE * MODEL_SIDE);
        for (channel, mean_std) in MEAN.iter().zip(STD).enumerate() {
            let expected = (128.0 / 255.0 - mean_std.0) / mean_std.1;
            let first = input[channel * MODEL_SIDE * MODEL_SIDE];
            let last = input[(channel + 1) * MODEL_SIDE * MODEL_SIDE - 1];
            assert!((first - expected).abs() < 1e-3 && (last - expected).abs() < 1e-3, "channel {channel}: {first} {last} {expected}");
        }
    }

    #[test]
    fn preprocessing_keeps_the_layout_of_the_picture() {
        // Red in the top left, blue at the bottom left: in the red plane the top left is bright, the bottom left dark.
        let input = preprocess(&asymmetric(200, 120));
        let red_top_left = input[20 * MODEL_SIDE + 20];
        let red_bottom_left = input[(MODEL_SIDE - 20) * MODEL_SIDE + 20];
        assert!(red_top_left > red_bottom_left, "{red_top_left} {red_bottom_left}");
        let blue = 2 * MODEL_SIDE * MODEL_SIDE;
        assert!(input[blue + (MODEL_SIDE - 20) * MODEL_SIDE + 20] > input[blue + 20 * MODEL_SIDE + 20]);
    }

    #[test]
    fn turning_a_picture_four_quarters_is_the_picture() {
        let picture = asymmetric(60, 40);
        assert!(same_picture(&turned(&turned(&turned(&turned(&picture, 1), 1), 1), 1), &picture));
        assert_eq!(turned(&picture, 1).dimensions(), (40, 60));
        assert!(same_picture(&turned(&picture, 2), &turned(&turned(&picture, 1), 1)));
        assert!(same_picture(&turned(&picture, 3), &turned(&turned(&turned(&picture, 1), 1), 1)));
    }

    #[test]
    fn turning_a_box_turns_its_crop_clockwise_by_the_quarters_asked() {
        // The box turn must do to the crop exactly what `turned` does to the picture, whatever the box's own angle.
        let scan = DynamicImage::ImageRgb8(asymmetric(400, 300));
        for angle in [0.0, 12.0, -35.0, 90.0, 180.0] {
            let rect = RotatedRect::new(200.0, 150.0, 240.0, 120.0, angle);
            let before = crop_region(&scan, &rect).unwrap().to_rgb8();
            for quarters in 0..4u8 {
                let after = crop_region(&scan, &turn_rect(&rect, quarters)).unwrap().to_rgb8();
                assert!(
                    same_picture(&after, &turned(&before, quarters)),
                    "angle {angle}, {quarters} quarter turns: {:?} against {:?}",
                    after.dimensions(),
                    turned(&before, quarters).dimensions()
                );
            }
        }
    }

    #[test]
    fn angles_stay_in_the_range_the_screen_uses() {
        for angle in [-170.0, -90.0, 0.0, 95.0, 179.0] {
            for quarters in 0..4 {
                let turned = turn_rect(&RotatedRect::new(0.0, 0.0, 10.0, 5.0, angle), quarters);
                assert!((-180.0..180.0).contains(&turned.angle), "{angle} + {quarters}: {}", turned.angle);
            }
        }
        // Turning four times is the box again.
        let rect = RotatedRect::new(5.0, 6.0, 30.0, 20.0, 33.0);
        let again = (0..4).fold(rect, |r, _| turn_rect(&r, 1));
        assert!((again.angle - rect.angle).abs() < 1e-9 && (again.w - rect.w).abs() < 1e-9);
    }
}
