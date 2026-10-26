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

#[cfg(test)]
mod tests {
    use super::*;

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
