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
/// along some hull edge, so try each edge and keep the smallest. `w` runs along
/// the edge direction `angle`. OpenCV's choice of which side is `w` (and the
/// 90 degree ambiguity of `angle`) differs by version; use [`RotatedRect::canonical`]
/// to compare.
pub fn min_area_rect(points: &[(f64, f64)]) -> RotatedRect {
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

impl RotatedRect {
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
        let line = min_area_rect(&[(0.0, 0.0), (6.0, 8.0)]);
        assert!((line.w - 10.0).abs() < 1e-9 && line.h == 0.0);
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
