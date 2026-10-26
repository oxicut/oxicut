//! Geometry primitives shared by the detectors.

/// A rotated rectangle in OpenCV `RotatedRect` convention: centre, size, and the
/// angle in degrees of the width axis, measured clockwise in image coordinates
/// (y pointing down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotatedRect {
    pub cx: f64,
    pub cy: f64,
    pub w: f64,
    pub h: f64,
    pub angle: f64,
}

impl RotatedRect {
    pub fn new(cx: f64, cy: f64, w: f64, h: f64, angle: f64) -> Self {
        Self {
            cx,
            cy,
            w,
            h,
            angle,
        }
    }

    pub fn area(&self) -> f64 {
        self.w * self.h
    }

    /// Scale centre and size by `factor`; the angle is unchanged. Used to map
    /// boxes found on the downsampled detection image back to source pixels.
    pub fn scaled(&self, factor: f64) -> Self {
        Self {
            cx: self.cx * factor,
            cy: self.cy * factor,
            w: self.w * factor,
            h: self.h * factor,
            angle: self.angle,
        }
    }

    /// The four corners computed in `f32` exactly like `cv::RotatedRect::points`,
    /// for the places where a pixel boundary decides the result (a `floor` of a
    /// corner that lies on an integer is only reproducible in single precision).
    pub fn corners_f32(&self) -> [(f32, f32); 4] {
        let (cx, cy, w, h) = (self.cx as f32, self.cy as f32, self.w as f32, self.h as f32);
        let angle = self.angle as f32 as f64 * std::f64::consts::PI / 180.0;
        let b = angle.cos() as f32 * 0.5;
        let a = angle.sin() as f32 * 0.5;
        // OpenCV 5 builds every corner from its own expression (older versions
        // mirrored the first two through the centre), which rounds differently.
        let p0 = (cx - a * h - b * w, cy + b * h - a * w);
        let p1 = (cx + a * h - b * w, cy - b * h - a * w);
        let p2 = (cx + a * h + b * w, cy - b * h + a * w);
        let p3 = (cx - a * h + b * w, cy + b * h + a * w);
        [p0, p1, p2, p3]
    }

    /// Tight integer bounding box `(x, y, width, height)` of the corners, as
    /// `cv2.boundingRect(cv2.boxPoints(rect))`: single-precision corners, with
    /// the far edge `floor`ed and included (hence the `+ 1`).
    pub fn bounding_rect(&self) -> (i64, i64, i64, i64) {
        let corners = self.corners_f32();
        let min_x = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
        let max_x = corners
            .iter()
            .map(|c| c.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let min_y = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min);
        let max_y = corners
            .iter()
            .map(|c| c.1)
            .fold(f32::NEG_INFINITY, f32::max);
        let (x, y) = (min_x.floor() as i64, min_y.floor() as i64);
        (
            x,
            y,
            max_x.floor() as i64 - x + 1,
            max_y.floor() as i64 - y + 1,
        )
    }

    /// The four corners, in the same order as OpenCV's `boxPoints`.
    pub fn corners(&self) -> [(f64, f64); 4] {
        let theta = self.angle.to_radians();
        let b = theta.cos() * 0.5;
        let a = theta.sin() * 0.5;
        let p0 = (
            self.cx - a * self.h - b * self.w,
            self.cy + b * self.h - a * self.w,
        );
        let p1 = (
            self.cx + a * self.h - b * self.w,
            self.cy - b * self.h - a * self.w,
        );
        let p2 = (2.0 * self.cx - p0.0, 2.0 * self.cy - p0.1);
        let p3 = (2.0 * self.cx - p1.0, 2.0 * self.cy - p1.1);
        [p0, p1, p2, p3]
    }
}

/// Convex hull (Andrew's monotone chain), counter-clockwise in a y-up frame,
/// collinear points dropped. Fewer than three distinct points come back as is.
pub fn convex_hull(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a.partial_cmp(b).expect("finite coordinates"));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    let mut hull: Vec<(f64, f64)> = Vec::with_capacity(pts.len() * 2);
    for &p in &pts {
        while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(p);
    }
    let lower_len = hull.len() + 1;
    for &p in pts.iter().rev().skip(1) {
        while hull.len() >= lower_len && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0
        {
            hull.pop();
        }
        hull.push(p);
    }
    hull.pop();
    hull
}

/// Minimum-area enclosing rotated rectangle (`cv2.minAreaRect`): one side lies
/// along some hull edge, so try each edge and keep the smallest. The result is
/// written the way OpenCV 5 writes it, see [`RotatedRect::opencv_normalized`],
/// because later detector stages behave differently for equivalent
/// descriptions of one rectangle.
pub fn min_area_rect(points: &[(f64, f64)]) -> RotatedRect {
    min_area_rect_raw(points).opencv_normalized()
}

fn min_area_rect_raw(points: &[(f64, f64)]) -> RotatedRect {
    let hull = convex_hull(points);
    match hull.len() {
        0 => return RotatedRect::new(0.0, 0.0, 0.0, 0.0, 0.0),
        1 => return RotatedRect::new(hull[0].0, hull[0].1, 0.0, 0.0, 0.0),
        2 => {
            let (a, b) = (hull[0], hull[1]);
            return RotatedRect::new(
                (a.0 + b.0) / 2.0,
                (a.1 + b.1) / 2.0,
                (b.0 - a.0).hypot(b.1 - a.1),
                0.0,
                (b.1 - a.1).atan2(b.0 - a.0).to_degrees(),
            );
        }
        _ => {}
    }
    let mut best: Option<(f64, RotatedRect)> = None;
    for i in 0..hull.len() {
        let origin = hull[i];
        let next = hull[(i + 1) % hull.len()];
        let length = (next.0 - origin.0).hypot(next.1 - origin.1);
        let u = ((next.0 - origin.0) / length, (next.1 - origin.1) / length);
        let v = (-u.1, u.0);
        let (mut min_u, mut max_u) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut min_v, mut max_v) = (f64::INFINITY, f64::NEG_INFINITY);
        for p in &hull {
            let (dx, dy) = (p.0 - origin.0, p.1 - origin.1);
            let pu = dx * u.0 + dy * u.1;
            let pv = dx * v.0 + dy * v.1;
            min_u = min_u.min(pu);
            max_u = max_u.max(pu);
            min_v = min_v.min(pv);
            max_v = max_v.max(pv);
        }
        let area = (max_u - min_u) * (max_v - min_v);
        if best.as_ref().is_none_or(|b| area < b.0) {
            let (mid_u, mid_v) = ((min_u + max_u) / 2.0, (min_v + max_v) / 2.0);
            best = Some((
                area,
                RotatedRect::new(
                    origin.0 + u.0 * mid_u + v.0 * mid_v,
                    origin.1 + u.1 * mid_u + v.1 * mid_v,
                    max_u - min_u,
                    max_v - min_v,
                    u.1.atan2(u.0).to_degrees(),
                ),
            ));
        }
    }
    best.expect("hull has edges").1
}

/// OpenCV's rotating-calipers search for the minimum-area rectangle, in
/// single precision exactly as `cv::rotatingCalipers(..., CALIPERS_MINAREARECT)`
/// runs it, so rounding matches. `hull` is a convex polygon (any orientation)
/// with at least three vertices; returns `(centre, width vector, height vector)`
/// before the final `sqrt`/`atan2`.
pub fn rotating_calipers(hull: &[(f32, f32)]) -> ((f32, f32), (f32, f32), (f32, f32)) {
    let n = hull.len();
    let points = hull;
    let mut vect = vec![(0f32, 0f32); n];
    let mut inv_length = vec![0f32; n];
    let (mut left, mut bottom, mut right, mut top) = (0usize, 0usize, 0usize, 0usize);

    let mut pt0 = points[0];
    let (mut left_x, mut right_x) = (pt0.0, pt0.0);
    let (mut top_y, mut bottom_y) = (pt0.1, pt0.1);
    for i in 0..n {
        if pt0.0 < left_x {
            left_x = pt0.0;
            left = i;
        }
        if pt0.0 > right_x {
            right_x = pt0.0;
            right = i;
        }
        if pt0.1 > top_y {
            top_y = pt0.1;
            top = i;
        }
        if pt0.1 < bottom_y {
            bottom_y = pt0.1;
            bottom = i;
        }
        let pt = points[(i + 1) % n];
        let (dx, dy) = ((pt.0 - pt0.0) as f64, (pt.1 - pt0.1) as f64);
        vect[i] = (dx as f32, dy as f32);
        inv_length[i] = (1.0 / (dx * dx + dy * dy).sqrt()) as f32;
        pt0 = pt;
    }

    // Orientation of the hull from the first non-degenerate corner.
    let mut orientation = 0f32;
    let (mut ax, mut ay) = (vect[n - 1].0 as f64, vect[n - 1].1 as f64);
    for v in &vect {
        let (bx, by) = (v.0 as f64, v.1 as f64);
        let convexity = ax * by - ay * bx;
        if convexity != 0.0 {
            orientation = if convexity > 0.0 { 1.0 } else { -1.0 };
            break;
        }
        ax = bx;
        ay = by;
    }
    let (mut base_a, mut base_b) = (orientation, 0f32);

    let mut seq = [bottom, right, top, left];
    let mut min_area = f32::MAX;
    // The best rectangle: (leftmost point index, base_a, width, base_b, height, bottom point index).
    let mut best = (0usize, 0f32, 0f32, 0f32, 0f32, 0usize);

    for _ in 0..n {
        let rot = [
            vect[seq[0]],
            (vect[seq[1]].1, -vect[seq[1]].0),
            (-vect[seq[2]].0, -vect[seq[2]].1),
            (-vect[seq[3]].1, vect[seq[3]].0),
        ];
        let mut main_element = 0;
        let mut max_cos = (base_a * rot[0].0 + base_b * rot[0].1) * inv_length[seq[0]];
        for i in 1..4 {
            let cos = (base_a * rot[i].0 + base_b * rot[i].1) * inv_length[seq[i]];
            if cos > max_cos {
                main_element = i;
                max_cos = cos;
            }
        }

        // Rotate the calipers onto the edge that is closest to aligned.
        let pindex = seq[main_element];
        let lead_x = vect[pindex].0 * inv_length[pindex];
        let lead_y = vect[pindex].1 * inv_length[pindex];
        (base_a, base_b) = match main_element {
            0 => (lead_x, lead_y),
            1 => (lead_y, -lead_x),
            2 => (-lead_x, -lead_y),
            _ => (-lead_y, lead_x),
        };
        seq[main_element] = (seq[main_element] + 1) % n;

        let dx = points[seq[1]].0 - points[seq[3]].0;
        let dy = points[seq[1]].1 - points[seq[3]].1;
        let width = dx * base_a + dy * base_b;
        let dx = points[seq[2]].0 - points[seq[0]].0;
        let dy = points[seq[2]].1 - points[seq[0]].1;
        let height = -dx * base_b + dy * base_a;
        let area = width * height;
        if area <= min_area {
            min_area = area;
            best = (seq[3], base_a, width, base_b, height, seq[0]);
        }
    }

    let (left_index, a1, width, b1, height, bottom_index) = best;
    let (a2, b2) = (-b1, a1);
    let left_point = points[left_index];
    let bottom_point = points[bottom_index];
    let c1 = a1 * left_point.0 + left_point.1 * b1;
    let c2 = a2 * bottom_point.0 + bottom_point.1 * b2;
    let inverse_det = 1.0f32 / (a1 * b2 - a2 * b1);
    let corner = (
        (c1 * b2 - c2 * b1) * inverse_det,
        (a1 * c2 - a2 * c1) * inverse_det,
    );
    (corner, (a1 * width, b1 * width), (a2 * height, b2 * height))
}

/// `cv::minAreaRect` for a convex `hull` with three or more vertices, in the
/// order and precision OpenCV uses. The result is *not* yet range-normalised.
pub fn min_area_rect_from_hull(hull: &[(f32, f32)]) -> RotatedRect {
    let (corner, width_vector, height_vector) = rotating_calipers(hull);
    let cx = corner.0 + (width_vector.0 + height_vector.0) * 0.5;
    let cy = corner.1 + (width_vector.1 + height_vector.1) * 0.5;
    let w = ((width_vector.0 as f64).powi(2) + (width_vector.1 as f64).powi(2)).sqrt() as f32;
    let h = ((height_vector.0 as f64).powi(2) + (height_vector.1 as f64).powi(2)).sqrt() as f32;
    let angle = (width_vector.1 as f64).atan2(width_vector.0 as f64) as f32;
    RotatedRect::new(
        cx as f64,
        cy as f64,
        w as f64,
        h as f64,
        (angle as f64 * 180.0 / std::f64::consts::PI) as f32 as f64,
    )
}

/// Signed area of a polygon (positive when counter-clockwise in a y-up frame).
fn signed_area(polygon: &[(f64, f64)]) -> f64 {
    let n = polygon.len();
    (0..n)
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum::<f64>()
        / 2.0
}

/// Area of the intersection of two rotated rectangles
/// (`cv2.rotatedRectangleIntersection` followed by `contourArea`).
/// Sutherland-Hodgman: clip one convex polygon by each edge of the other.
pub fn intersection_area(a: &RotatedRect, b: &RotatedRect) -> f64 {
    if a.area() <= 0.0 || b.area() <= 0.0 {
        return 0.0;
    }
    let mut subject = a.corners().to_vec();
    let clip = b.corners();
    // Keep the side of each clip edge that the clip polygon's interior is on.
    let side = signed_area(&clip).signum();
    for i in 0..4 {
        let (p, q) = (clip[i], clip[(i + 1) % 4]);
        let inside = |pt: (f64, f64)| {
            side * ((q.0 - p.0) * (pt.1 - p.1) - (q.1 - p.1) * (pt.0 - p.0)) >= 0.0
        };
        let crossing = |s: (f64, f64), e: (f64, f64)| {
            let (d1, d2) = (
                (q.0 - p.0) * (s.1 - p.1) - (q.1 - p.1) * (s.0 - p.0),
                (q.0 - p.0) * (e.1 - p.1) - (q.1 - p.1) * (e.0 - p.0),
            );
            let t = d1 / (d1 - d2);
            (s.0 + t * (e.0 - s.0), s.1 + t * (e.1 - s.1))
        };
        let input = std::mem::take(&mut subject);
        for j in 0..input.len() {
            let (start, end) = (input[j], input[(j + 1) % input.len()]);
            match (inside(start), inside(end)) {
                (true, true) => subject.push(end),
                (true, false) => subject.push(crossing(start, end)),
                (false, true) => {
                    subject.push(crossing(start, end));
                    subject.push(end);
                }
                (false, false) => {}
            }
        }
        if subject.is_empty() {
            return 0.0;
        }
    }
    signed_area(&subject).abs()
}

/// Intersection over the smaller rectangle's area (`_v3_overlap_fraction`).
pub fn overlap_fraction(a: &RotatedRect, b: &RotatedRect) -> f64 {
    intersection_area(a, b) / a.area().min(b.area()).max(1.0)
}

/// Intersection over union (`_v3_iou`).
pub fn iou(a: &RotatedRect, b: &RotatedRect) -> f64 {
    let intersection = intersection_area(a, b);
    intersection / (a.area() + b.area() - intersection).max(1.0)
}

impl RotatedRect {
    /// The rectangle as OpenCV 5's `minAreaRect` reports it: angle in
    /// `[-90, 0)` (an axis-aligned box is always `-90`), with width and height
    /// swapped when that needs an odd number of quarter turns, and every number
    /// rounded to `f32`. Over an interval of only 90 degrees the description
    /// of a given shape is unique.
    pub fn opencv_normalized(&self) -> Self {
        let angle = self.angle.rem_euclid(90.0) - 90.0;
        let quarter_turns = ((angle - self.angle) / 90.0).round() as i64;
        let (w, h) = if quarter_turns.rem_euclid(2) == 1 {
            (self.h, self.w)
        } else {
            (self.w, self.h)
        };
        let f = |v: f64| v as f32 as f64;
        Self::new(f(self.cx), f(self.cy), f(w), f(h), f(angle))
    }

    /// Same rectangle with `w >= h` and the angle in `[0, 180)`, so two
    /// descriptions of one rectangle compare equal.
    pub fn canonical(&self) -> Self {
        let (mut w, mut h, mut angle) = (self.w, self.h, self.angle);
        if w < h {
            std::mem::swap(&mut w, &mut h);
            angle += 90.0;
        }
        Self {
            cx: self.cx,
            cy: self.cy,
            w,
            h,
            angle: angle.rem_euclid(180.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rotated_square_corners(side: f64, degrees: f64) -> Vec<(f64, f64)> {
        let rect = RotatedRect::new(50.0, 40.0, side, side, degrees);
        rect.corners().to_vec()
    }

    #[test]
    fn hull_drops_interior_and_collinear_points() {
        let points = [
            (0.0, 0.0),
            (4.0, 0.0),
            (2.0, 0.0),
            (4.0, 4.0),
            (0.0, 4.0),
            (2.0, 2.0),
        ];
        let hull = convex_hull(&points);
        assert_eq!(hull.len(), 4);
        for corner in [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)] {
            assert!(hull.contains(&corner), "{corner:?} missing from {hull:?}");
        }
    }

    #[test]
    fn min_area_rect_of_axis_aligned_box() {
        let points = [
            (10.0, 20.0),
            (50.0, 20.0),
            (50.0, 30.0),
            (10.0, 30.0),
            (30.0, 25.0),
        ];
        let rect = min_area_rect(&points).canonical();
        assert!((rect.cx - 30.0).abs() < 1e-9 && (rect.cy - 25.0).abs() < 1e-9);
        assert!((rect.w - 40.0).abs() < 1e-9 && (rect.h - 10.0).abs() < 1e-9);
        assert!(rect.angle.abs() < 1e-9 || (rect.angle - 180.0).abs() < 1e-9);
    }

    #[test]
    fn min_area_rect_recovers_a_rotated_rectangle() {
        let truth = RotatedRect::new(120.0, 80.0, 90.0, 30.0, 23.0);
        let rect = min_area_rect(&truth.corners()).canonical();
        assert!((rect.cx - 120.0).abs() < 1e-9 && (rect.cy - 80.0).abs() < 1e-9);
        assert!((rect.w - 90.0).abs() < 1e-9 && (rect.h - 30.0).abs() < 1e-9);
        assert!((rect.angle - 23.0).abs() < 1e-9, "angle {}", rect.angle);
    }

    #[test]
    fn min_area_rect_of_a_tilted_square_is_the_square() {
        let rect = min_area_rect(&rotated_square_corners(10.0, 30.0));
        assert!((rect.area() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        assert_eq!(min_area_rect(&[]).area(), 0.0);
        assert_eq!(min_area_rect(&[(3.0, 4.0)]).area(), 0.0);
        let line = min_area_rect(&[(0.0, 0.0), (6.0, 8.0)]).canonical();
        assert!((line.w - 10.0).abs() < 1e-9 && line.h == 0.0);
    }

    #[test]
    fn intersection_of_axis_aligned_boxes() {
        let a = RotatedRect::new(50.0, 50.0, 100.0, 100.0, 0.0);
        let b = RotatedRect::new(100.0, 50.0, 100.0, 100.0, 0.0);
        assert!((intersection_area(&a, &b) - 5000.0).abs() < 1e-6);
        assert!((overlap_fraction(&a, &b) - 0.5).abs() < 1e-9);
        assert!((iou(&a, &b) - 5000.0 / 15000.0).abs() < 1e-9);
    }

    #[test]
    fn intersection_edge_cases() {
        let a = RotatedRect::new(0.0, 0.0, 10.0, 10.0, 0.0);
        let far = RotatedRect::new(100.0, 0.0, 10.0, 10.0, 0.0);
        assert_eq!(intersection_area(&a, &far), 0.0);
        let inside = RotatedRect::new(0.0, 0.0, 4.0, 4.0, 30.0);
        assert!((intersection_area(&a, &inside) - 16.0).abs() < 1e-6);
        assert!((intersection_area(&inside, &a) - 16.0).abs() < 1e-6);
        assert!((intersection_area(&a, &a) - 100.0).abs() < 1e-6);
    }

    #[test]
    fn rotated_square_over_square_is_an_octagon() {
        let a = RotatedRect::new(0.0, 0.0, 10.0, 10.0, 0.0);
        let b = RotatedRect::new(0.0, 0.0, 10.0, 10.0, 45.0);
        // Regular octagon inscribed in both: 2(sqrt2 - 1) * 10^2 * ... computed directly.
        let expected = 2.0 * (2.0_f64.sqrt() - 1.0) * 100.0 / 1.0 * 1.0;
        assert!(
            (intersection_area(&a, &b) - expected).abs() < 1e-6,
            "{}",
            intersection_area(&a, &b)
        );
    }

    #[test]
    fn calipers_find_the_minimum_rectangle_of_simple_hulls() {
        // Axis-aligned, counter-clockwise in a y-up frame.
        let rect = [(0.0f32, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)];
        let found = min_area_rect_from_hull(&rect).canonical();
        assert!(
            (found.w - 10.0).abs() < 1e-4 && (found.h - 5.0).abs() < 1e-4,
            "{found:?}"
        );
        assert!(
            (found.cx - 5.0).abs() < 1e-4 && (found.cy - 2.5).abs() < 1e-4,
            "{found:?}"
        );

        // A tilted rectangle, given as a hull of its corners.
        let truth = RotatedRect::new(50.0, 40.0, 30.0, 12.0, 20.0);
        let hull: Vec<(f64, f64)> = convex_hull(&truth.corners());
        let corners: Vec<(f32, f32)> = hull.iter().map(|&(x, y)| (x as f32, y as f32)).collect();
        let found = min_area_rect_from_hull(&corners).canonical();
        assert!(
            (found.w - 30.0).abs() < 1e-3 && (found.h - 12.0).abs() < 1e-3,
            "{found:?}"
        );
        assert!((found.angle - 20.0).abs() < 1e-3, "{found:?}");
    }

    #[test]
    fn opencv_normalisation_puts_the_angle_in_minus_90_to_0() {
        // An axis-aligned box is always -90, swapping the sides from angle 0.
        let upright = RotatedRect::new(10.0, 20.0, 40.0, 30.0, 0.0).opencv_normalized();
        assert_eq!((upright.w, upright.h, upright.angle), (30.0, 40.0, -90.0));
        let already = RotatedRect::new(10.0, 20.0, 40.0, 30.0, -90.0).opencv_normalized();
        assert_eq!((already.w, already.h, already.angle), (40.0, 30.0, -90.0));
        // A small tilt keeps its sides; the opposite tilt wraps by a quarter turn.
        let tilt = RotatedRect::new(0.0, 0.0, 40.0, 30.0, -3.5).opencv_normalized();
        assert_eq!((tilt.w, tilt.h, tilt.angle), (40.0, 30.0, -3.5));
        let wrapped = RotatedRect::new(0.0, 0.0, 40.0, 30.0, 3.5).opencv_normalized();
        assert_eq!((wrapped.w, wrapped.h, wrapped.angle), (30.0, 40.0, -86.5));
        // Equivalent descriptions normalise identically.
        let a = RotatedRect::new(5.0, 5.0, 40.0, 30.0, 20.0).opencv_normalized();
        let b = RotatedRect::new(5.0, 5.0, 30.0, 40.0, 110.0).opencv_normalized();
        let c = RotatedRect::new(5.0, 5.0, 40.0, 30.0, -160.0).opencv_normalized();
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn canonical_orders_sides_and_wraps_angle() {
        let r = RotatedRect::new(0.0, 0.0, 10.0, 30.0, -20.0).canonical();
        assert_eq!((r.w, r.h), (30.0, 10.0));
        assert!((r.angle - 70.0).abs() < 1e-9);
    }

    #[test]
    fn corners_of_axis_aligned_rect() {
        let rect = RotatedRect::new(10.0, 20.0, 4.0, 2.0, 0.0);
        let corners = rect.corners();
        // boxPoints order for angle 0: bottom-left, top-left, top-right, bottom-right.
        assert_eq!(
            corners,
            [(8.0, 21.0), (8.0, 19.0), (12.0, 19.0), (12.0, 21.0)]
        );
    }

    #[test]
    fn scaled_keeps_angle() {
        let rect = RotatedRect::new(10.0, 20.0, 4.0, 2.0, 12.5).scaled(2.0);
        assert_eq!(rect, RotatedRect::new(20.0, 40.0, 8.0, 4.0, 12.5));
    }
}
