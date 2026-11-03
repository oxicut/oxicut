//! Other ways to cut one picture.
//!
//! The detectors make several candidate boxes for every print and keep only the winner (v7's
//! `_select`). This keeps the runners-up from every source (v3, the whole-card pass, v3 with a colour
//! cap, the chroma proposals and, with the models, MobileSAM), each as found and moved onto the
//! picture's own edge, and picks the ones that are about the same print as a box the user is looking at.
//! ScanSplitter has nothing like it.

use image::RgbImage;

use crate::chroma::chroma_rectangles;
use crate::detect::{CutTo, DetectOptions, detect_cards, detect_v3, detect_v3_capped, regions_from_rects};
use crate::geometry::{RotatedRect, iou, overlap_fraction};
use crate::orient::turn_rect;
use crate::pil::working_gray;
use crate::sam::{MobileSam, SamError};
use crate::score::score_rect;
use crate::tighten::{GrayF32, tighten_rect};
use crate::v5;
use crate::v7::{CAPPED_COLOR_THRESHOLD, PASSES, WORK_MAX, blurred, v3_proposals};

/// A candidate box in full-resolution pixels, before any trim, with the evidence v7 judges it by.
#[derive(Debug, Clone, Copy)]
pub struct Proposal {
    pub rect: RotatedRect,
    pub score: f64,
}

/// A candidate has to be about the same print: the smaller of the two mostly inside the other, their centres
/// close (a box over two neighbouring prints has its centre in the gap, a card its artwork about in the
/// middle), and not wildly bigger or smaller. A card cut whole is a few times the area of its artwork, so
/// the limits are wide.
const MIN_CONTAINED: f64 = 0.7;
/// Of the longer side of the larger box.
const MAX_CENTRE_APART: f64 = 0.35;
const MIN_AREA_RATIO: f64 = 0.3;
const MAX_AREA_RATIO: f64 = 3.5;
/// Closer than this to the box already shown, or to a better suggestion, is the same suggestion.
const SAME_AS_CURRENT: f64 = 0.95;
const SAME_AS_OTHER: f64 = 0.9;

/// Every candidate box for the prints on `source`, as found and tightened, best scores first. The models
/// are only used when `sam` is given.
pub fn proposals(
    source: &RgbImage,
    options: &DetectOptions,
    sam: Option<&mut MobileSam>,
) -> Result<Vec<Proposal>, SamError> {
    let picture_options = DetectOptions {
        cut_to: CutTo::Picture,
        ..*options
    };
    let classic = detect_v3(source, &picture_options);
    let v3 = v3_proposals(&classic, source, options);
    let mut sets: Vec<Vec<RotatedRect>> = vec![v3.clone()];
    if let Some(cards) = detect_cards(source, options) {
        sets.push(v3_proposals(&cards, source, options));
    }
    let capped = detect_v3_capped(source, &picture_options, Some(CAPPED_COLOR_THRESHOLD));
    sets.push(v3_proposals(&capped, source, options));
    sets.push(chroma_rectangles(source, options.min_area_ratio, options.max_area_ratio));
    if let Some(sam) = sam {
        sets.push(v5::rectangles(
            sam,
            source,
            &v3,
            options.min_area_ratio,
            options.max_area_ratio,
        )?);
    }

    let (work, scale) = working_gray(source, WORK_MAX);
    let work = GrayF32::from_gray(&work);
    let smooth = blurred(&work, 1.0);
    let mut out: Vec<Proposal> = Vec::new();
    for rect in sets.into_iter().flatten() {
        let tight = tighten_rect(&work, &rect, scale, PASSES);
        for rect in [rect, tight] {
            out.push(Proposal {
                rect,
                score: score_rect(&smooth, &rect, scale),
            });
        }
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    Ok(out)
}

/// Whether `other` is about the same print as `current`.
fn same_print(current: &RotatedRect, other: &RotatedRect) -> bool {
    let ratio = other.area() / current.area().max(1.0);
    let larger = if other.area() > current.area() { other } else { current };
    let apart = (other.cx - current.cx).hypot(other.cy - current.cy);
    (MIN_AREA_RATIO..=MAX_AREA_RATIO).contains(&ratio)
        && overlap_fraction(current, other) >= MIN_CONTAINED
        && apart <= MAX_CENTRE_APART * larger.w.max(larger.h)
}

/// The angle in `[-180, 180)`.
fn normalise(angle: f64) -> f64 {
    (angle + 180.0).rem_euclid(360.0) - 180.0
}

/// `rect` turned so that its "up" is the same as `like`'s: of the four turns of the same rectangle, the
/// one whose angle is nearest. A picture the user (or the orientation helper) turned never flips when
/// another cut is tried.
fn facing_like(rect: &RotatedRect, like: &RotatedRect) -> RotatedRect {
    (0..4u8)
        .map(|q| turn_rect(rect, q))
        .min_by(|a, b| {
            let da = normalise(a.angle - like.angle).abs();
            let db = normalise(b.angle - like.angle).abs();
            da.total_cmp(&db)
        })
        .expect("four turns")
}

/// The other ways to cut the print `current` is on, best first and at most `limit`. The trim and padding
/// in `options` apply as they do to a detected box (so a candidate is compared with `current` as the user
/// sees it), and every suggestion faces the way `current` does.
pub fn suggestions_for(
    current: &RotatedRect,
    proposals: &[Proposal],
    options: &DetectOptions,
    width: u32,
    height: u32,
    limit: usize,
) -> Vec<RotatedRect> {
    let mut ordered: Vec<&Proposal> = proposals.iter().collect();
    ordered.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut out: Vec<RotatedRect> = Vec::new();
    for proposal in ordered {
        let Some(region) = regions_from_rects(&[proposal.rect], width, height, options).into_iter().next() else {
            continue;
        };
        let rect = facing_like(&region.rect, current);
        if !same_print(current, &rect) {
            continue;
        }
        if iou(current, &rect) >= SAME_AS_CURRENT || out.iter().any(|o| iou(o, &rect) >= SAME_AS_OTHER) {
            continue;
        }
        out.push(rect);
        if out.len() == limit {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn options() -> DetectOptions {
        DetectOptions {
            inset: 0,
            ..Default::default()
        }
    }

    fn proposal(cx: f64, cy: f64, w: f64, h: f64, angle: f64, score: f64) -> Proposal {
        Proposal {
            rect: RotatedRect::new(cx, cy, w, h, angle),
            score,
        }
    }

    fn near(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    #[test]
    fn candidates_that_are_the_same_print_come_best_first() {
        let current = RotatedRect::new(500.0, 350.0, 500.0, 400.0, 0.0);
        let found = [
            proposal(500.0, 350.0, 468.0, 368.0, 0.0, 30.0),
            proposal(500.0, 350.0, 520.0, 420.0, 0.0, 40.0),
            proposal(500.0, 350.0, 500.0, 400.0, 0.0, 50.0),
        ];
        let out = suggestions_for(&current, &found, &options(), 1000, 700, 6);
        assert_eq!(out.len(), 2, "the one like the current box is left out: {out:?}");
        assert!(near(out[0].w, 520.0, 0.5), "best score first: {out:?}");
        assert!(near(out[1].w, 468.0, 0.5));
    }

    #[test]
    fn a_whole_card_is_offered_for_its_artwork_and_a_box_over_two_prints_is_not() {
        let artwork = RotatedRect::new(300.0, 300.0, 300.0, 200.0, 0.0);
        let found = [
            proposal(300.0, 300.0, 420.0, 320.0, 0.0, 50.0),
            // Over this print and its neighbour: its centre is in the gap between them.
            proposal(540.0, 300.0, 600.0, 220.0, 0.0, 60.0),
        ];
        let out = suggestions_for(&artwork, &found, &options(), 1000, 700, 6);
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(near(out[0].w, 420.0, 0.5));
    }

    #[test]
    fn other_prints_and_much_bigger_boxes_are_not_offered() {
        let current = RotatedRect::new(300.0, 300.0, 300.0, 200.0, 0.0);
        let found = [
            proposal(700.0, 300.0, 300.0, 200.0, 0.0, 60.0),
            proposal(300.0, 300.0, 900.0, 600.0, 0.0, 60.0),
            proposal(300.0, 300.0, 60.0, 40.0, 0.0, 60.0),
        ];
        assert!(suggestions_for(&current, &found, &options(), 1000, 700, 6).is_empty());
    }

    #[test]
    fn near_duplicates_collapse_and_the_list_is_capped() {
        let current = RotatedRect::new(500.0, 350.0, 500.0, 400.0, 0.0);
        let found = [
            proposal(500.0, 350.0, 468.0, 368.0, 0.0, 30.0),
            proposal(500.5, 350.0, 468.0, 368.0, 0.2, 29.0),
            proposal(500.0, 350.0, 440.0, 340.0, 0.0, 20.0),
            proposal(500.0, 350.0, 410.0, 310.0, 0.0, 10.0),
        ];
        let out = suggestions_for(&current, &found, &options(), 1000, 700, 6);
        assert_eq!(out.len(), 3);
        assert_eq!(suggestions_for(&current, &found, &options(), 1000, 700, 2).len(), 2);
    }

    #[test]
    fn a_picture_turned_a_quarter_keeps_its_turn_in_every_suggestion() {
        // The user's box is the card on its side: sides swapped, angle down a quarter.
        let upright = RotatedRect::new(500.0, 350.0, 500.0, 400.0, 0.0);
        for q in 0..4u8 {
            let current = turn_rect(&upright, q);
            let out = suggestions_for(
                &current,
                &[proposal(500.0, 350.0, 468.0, 368.0, 0.0, 30.0)],
                &options(),
                1000,
                700,
                6,
            );
            assert_eq!(out.len(), 1);
            let (a, b) = (out[0], turn_rect(&RotatedRect::new(500.0, 350.0, 468.0, 368.0, 0.0), q));
            assert!(
                near(normalise(a.angle - b.angle), 0.0, 0.01) && near(a.w, b.w, 0.01) && near(a.h, b.h, 0.01),
                "turn {q}: {a:?} against {b:?}"
            );
        }
    }

    #[test]
    fn the_trim_applies_to_a_suggestion_as_it_does_to_a_detected_box() {
        let current = RotatedRect::new(500.0, 350.0, 480.0, 380.0, 0.0);
        let trimmed = DetectOptions {
            inset: 10,
            ..Default::default()
        };
        let out = suggestions_for(
            &current,
            &[proposal(500.0, 350.0, 520.0, 420.0, 0.0, 30.0)],
            &trimmed,
            1000,
            700,
            6,
        );
        assert_eq!(out.len(), 1);
        assert!(near(out[0].w, 500.0, 0.5) && near(out[0].h, 400.0, 0.5), "{:?}", out[0]);
    }

    /// White page with a print: a mid-grey border (130, clearly not page) 16 px thick around a darker picture.
    fn page_with_bordered_print() -> RgbImage {
        let mut image = RgbImage::from_pixel(1000, 700, Rgb([250, 250, 250]));
        for y in 150..550 {
            for x in 200..700 {
                let inside = (216..684).contains(&x) && (166..534).contains(&y);
                let noise = ((x * 7 + y * 13) % 9) as u8;
                image.put_pixel(
                    x,
                    y,
                    if inside {
                        Rgb([30 + noise, 40, 70])
                    } else {
                        Rgb([130, 130, 130])
                    },
                );
            }
        }
        image
    }

    #[test]
    fn on_a_real_page_the_whole_print_and_the_picture_are_each_others_alternatives() {
        let page = page_with_bordered_print();
        let found = proposals(&page, &options(), None).unwrap();
        assert!(!found.is_empty());

        let whole = RotatedRect::new(450.0, 350.0, 500.0, 400.0, 0.0);
        let picture = RotatedRect::new(450.0, 350.0, 468.0, 368.0, 0.0);
        let from_whole = suggestions_for(&whole, &found, &options(), 1000, 700, 6);
        assert!(
            from_whole.iter().any(|r| near(r.w, 468.0, 8.0) && near(r.h, 368.0, 8.0)),
            "{from_whole:?}"
        );
        let from_picture = suggestions_for(&picture, &found, &options(), 1000, 700, 6);
        assert!(
            from_picture.iter().any(|r| near(r.w, 500.0, 8.0) && near(r.h, 400.0, 8.0)),
            "{from_picture:?}"
        );
        assert!(from_whole.len() <= 6 && from_picture.len() <= 6);
    }

    #[test]
    fn an_empty_part_of_the_page_gets_nothing() {
        let page = page_with_bordered_print();
        let found = proposals(&page, &options(), None).unwrap();
        let nowhere = RotatedRect::new(900.0, 640.0, 120.0, 90.0, 0.0);
        assert!(suggestions_for(&nowhere, &found, &options(), 1000, 700, 6).is_empty());
    }
}
