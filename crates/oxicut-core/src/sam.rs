//! MobileSAM through ONNX Runtime, driven the way ScanSplitter's `_v4_refine_rectangles`
//! drives it: the scan is shrunk to 1024 px on its long side, encoded once, and each
//! proposal is a box prompt to the mask decoder.

use std::path::Path;
use std::sync::OnceLock;

use image::RgbImage;
use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;

use crate::contours::{contour_area, find_external_contours};
use crate::geometry::{RotatedRect, min_area_rect, overlap_fraction};
use crate::resize::resize_area;

/// Long side the encoder is fed.
const ENCODER_SIDE: f64 = 1024.0;

#[derive(Debug, thiserror::Error)]
pub enum SamError {
    #[error("MobileSAM: {0}")]
    Runtime(String),
}

fn rt(error: impl std::fmt::Display) -> SamError {
    SamError::Runtime(error.to_string())
}

/// ONNX Runtime allows one environment per process.
fn environment() -> Result<&'static Environment, SamError> {
    static ENV: OnceLock<Result<Environment, String>> = OnceLock::new();
    ENV.get_or_init(|| ort::init().build().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| SamError::Runtime(e.clone()))
}

pub struct MobileSam {
    encoder: Session,
    decoder: Session,
}

/// The encoder's output for one scan plus the geometry needed to prompt it.
pub struct Embedding {
    shape: Vec<i64>,
    data: Vec<f32>,
    /// Factor from source pixels to the encoder image.
    pub scale: f64,
    /// Encoder image size.
    pub width: u32,
    pub height: u32,
    /// Source image size.
    pub source_width: u32,
    pub source_height: u32,
}

impl Embedding {
    pub fn shape(&self) -> &[i64] {
        &self.shape
    }

    pub fn data(&self) -> &[f32] {
        &self.data
    }
}

/// One decoded mask on the encoder image, with the decoder's quality estimate.
pub struct Mask {
    pub width: u32,
    pub height: u32,
    /// `true` where the logit is above zero.
    pub inside: Vec<bool>,
    pub predicted_iou: f32,
}

impl MobileSam {
    pub fn load(encoder: &Path, decoder: &Path) -> Result<Self, SamError> {
        let build = |path: &Path| -> Result<Session, SamError> {
            Session::builder(environment()?)
                .map_err(rt)?
                .commit_from_file(path)
                .map_err(rt)
        };
        Ok(Self {
            encoder: build(encoder)?,
            decoder: build(decoder)?,
        })
    }

    /// Shrink to 1024 px (never enlarge), run the encoder.
    pub fn embed(&mut self, rgb: &RgbImage) -> Result<Embedding, SamError> {
        let (source_width, source_height) = rgb.dimensions();
        let scale = (ENCODER_SIDE / source_width.max(source_height) as f64).min(1.0);
        let width = ((source_width as f64 * scale).round() as u32).max(1);
        let height = ((source_height as f64 * scale).round() as u32).max(1);
        let resized = resize_area(rgb, width, height);
        let pixels: Vec<f32> = resized.as_raw().iter().map(|&v| v as f32).collect();
        let input =
            Tensor::from_array((vec![height as i64, width as i64, 3], pixels)).map_err(rt)?;
        let outputs = self
            .encoder
            .run(ort::inputs!["input_image" => input])
            .map_err(rt)?;
        let (shape, data) = outputs[0].try_extract_tensor::<f32>().map_err(rt)?;
        Ok(Embedding {
            shape: shape.iter().copied().collect(),
            data: data.to_vec(),
            scale,
            width,
            height,
            source_width,
            source_height,
        })
    }

    /// Mask for a box prompt `(x1, y1, x2, y2)` given in encoder-image pixels.
    pub fn mask(&mut self, embedding: &Embedding, prompt: [f32; 4]) -> Result<Mask, SamError> {
        let tensor =
            |shape: Vec<i64>, data: Vec<f32>| Tensor::from_array((shape, data)).map_err(rt);
        let outputs = self
            .decoder
            .run(ort::inputs![
                "image_embeddings" => tensor(embedding.shape.clone(), embedding.data.clone())?,
                "point_coords" => tensor(vec![1, 2, 2], prompt.to_vec())?,
                "point_labels" => tensor(vec![1, 2], vec![2.0, 3.0])?,
                "mask_input" => tensor(vec![1, 1, 256, 256], vec![0.0; 256 * 256])?,
                "has_mask_input" => tensor(vec![1], vec![0.0])?,
                "orig_im_size" => tensor(vec![2], vec![embedding.height as f32, embedding.width as f32])?,
            ])
            .map_err(rt)?;
        let (mask_shape, logits) = outputs[0].try_extract_tensor::<f32>().map_err(rt)?;
        let (_, iou) = outputs[1].try_extract_tensor::<f32>().map_err(rt)?;
        let (height, width) = (
            mask_shape[mask_shape.len() - 2] as u32,
            mask_shape[mask_shape.len() - 1] as u32,
        );
        // masks[0, 0]: the first plane.
        let plane = (width * height) as usize;
        Ok(Mask {
            width,
            height,
            inside: logits[..plane].iter().map(|&v| v > 0.0).collect(),
            predicted_iou: iou[0],
        })
    }

    /// `_v4_refine_rectangles`: replace each proposal (source pixels) by the rectangle of
    /// its box-prompted mask, or keep it when the mask is not credible.
    pub fn refine_rectangles(
        &mut self,
        rgb: &RgbImage,
        proposals: &[RotatedRect],
        prompt_margin_ratio: f64,
    ) -> Result<Vec<RotatedRect>, SamError> {
        if proposals.is_empty() {
            return Ok(Vec::new());
        }
        let embedding = self.embed(rgb)?;
        proposals
            .iter()
            .map(|proposal| {
                let prompt = box_prompt(&embedding, proposal, prompt_margin_ratio);
                let mask = self.mask(&embedding, prompt)?;
                let small = proposal.scaled(embedding.scale);
                Ok(match mask_rect(&mask, &small) {
                    Some(rect) => rect.scaled(1.0 / embedding.scale),
                    None => *proposal,
                })
            })
            .collect()
    }
}

/// The decoder's box prompt for a proposal: its bounds plus a margin, clamped to the
/// source image, then scaled onto the encoder image.
pub fn box_prompt(embedding: &Embedding, proposal: &RotatedRect, margin_ratio: f64) -> [f32; 4] {
    let corners = proposal.corners_f32();
    let (mut x1, mut y1) = (f32::MAX, f32::MAX);
    let (mut x2, mut y2) = (f32::MIN, f32::MIN);
    for (x, y) in corners {
        x1 = x1.min(x);
        y1 = y1.min(y);
        x2 = x2.max(x);
        y2 = y2.max(y);
    }
    let margin = margin_ratio as f32 * (x2 - x1).min(y2 - y1);
    let scale = embedding.scale as f32;
    [
        (x1 - margin).max(0.0) * scale,
        (y1 - margin).max(0.0) * scale,
        (x2 + margin).min(embedding.source_width as f32 - 1.0) * scale,
        (y2 + margin).min(embedding.source_height as f32 - 1.0) * scale,
    ]
}

/// `_v4_mask_rect`: the most print-like rectangle among a mask's outlines, if any is
/// credible. `proposal` is in the mask's pixel space.
pub fn mask_rect(mask: &Mask, proposal: &RotatedRect) -> Option<RotatedRect> {
    if mask.predicted_iou < 0.70 {
        return None;
    }
    let binary = image::GrayImage::from_fn(mask.width, mask.height, |x, y| {
        image::Luma([u8::from(mask.inside[(y * mask.width + x) as usize])])
    });
    let proposal_area = proposal.area();
    let mut best: Option<(f64, RotatedRect)> = None;
    for contour in find_external_contours(&binary) {
        let area = contour_area(&contour);
        if area < 100.0 {
            continue;
        }
        let points: Vec<(f64, f64)> = contour.iter().map(|&(x, y)| (x as f64, y as f64)).collect();
        let rect = min_area_rect(&points);
        let rect_area = rect.w * rect.h;
        if rect_area <= 0.0 {
            continue;
        }
        let area_change = rect_area / proposal_area.max(1.0);
        if !(0.25..=2.50).contains(&area_change) {
            continue;
        }
        let rectangularity = area / rect_area;
        if rectangularity < 0.62 {
            continue;
        }
        let aspect = rect.w.max(rect.h) / rect.w.min(rect.h).max(1.0);
        if aspect > 5.0 {
            continue;
        }
        let overlap = overlap_fraction(&rect, proposal);
        if overlap < 0.65 {
            continue;
        }
        let score = rectangularity + 0.5 * overlap + 0.15 * area_change.min(1.0);
        if best.is_none_or(|(top, _)| score > top) {
            best = Some((score, rect));
        }
    }
    best.map(|(_, rect)| rect)
}

thread_local! {
    static THREAD_SAM: std::cell::RefCell<Option<(std::path::PathBuf, std::path::PathBuf, MobileSam)>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `f` with this thread's MobileSAM sessions, loading them on first use. A session
/// needs exclusive access to run, so each worker thread keeps its own.
pub fn with_thread_sam<R>(
    encoder: &Path,
    decoder: &Path,
    f: impl FnOnce(&mut MobileSam) -> R,
) -> Result<R, SamError> {
    THREAD_SAM.with(|cell| {
        let mut slot = cell.borrow_mut();
        let current = slot
            .as_ref()
            .is_some_and(|(e, d, _)| e == encoder && d == decoder);
        if !current {
            *slot = Some((
                encoder.to_path_buf(),
                decoder.to_path_buf(),
                MobileSam::load(encoder, decoder)?,
            ));
        }
        let (_, _, sam) = slot.as_mut().expect("loaded above");
        Ok(f(sam))
    })
}
