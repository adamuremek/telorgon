use crate::core::{PointF, RectF};
use crate::ui::{Border, CornerRadii};

/// An analytic rounded clip in output-pixel coordinates, independent of a retained scene.
/// Composite placements can intersect two such bounds without rewriting client pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundedClip {
    pub rect: RectF,
    pub radii: CornerRadii,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ColorRgba8;

    #[test]
    fn inner_curve_is_concentric_with_uniform_border_and_clamps_when_thick() {
        let outer = RoundedClip::new(
            RectF {
                x: 20.0,
                y: 30.0,
                width: 100.0,
                height: 80.0,
            },
            CornerRadii::all(12.0),
        );
        for width in [0.0, 2.0, 12.0, 20.0] {
            let inner = outer.inset(Border::all(width, ColorRgba8::rgba(0, 0, 0, 255)));
            assert_eq!(inner.rect.x, 20.0 + width);
            assert_eq!(inner.rect.y, 30.0 + width);
            assert_eq!(inner.rect.width, 100.0 - width * 2.0);
            assert_eq!(inner.radii, CornerRadii::all((12.0 - width).max(0.0)));
            if width <= 12.0 {
                assert_eq!(
                    inner.rect.x + inner.radii.top_left,
                    outer.rect.x + outer.radii.top_left
                );
            }
        }
    }

    #[test]
    fn clip_coverage_handles_square_empty_oversized_and_translated_bounds() {
        let rect = RectF {
            x: -20.0,
            y: 10.0,
            width: 20.0,
            height: 10.0,
        };
        let square = RoundedClip::new(rect, CornerRadii::default());
        assert_eq!(square.coverage(PointF { x: -19.5, y: 10.5 }), 1.0);
        let rounded = RoundedClip::new(rect, CornerRadii::all(1000.0));
        assert_eq!(rounded.radii, CornerRadii::all(5.0));
        assert_eq!(rounded.coverage(PointF { x: -19.5, y: 10.5 }), 0.0);
        assert_eq!(rounded.coverage(PointF { x: -10.0, y: 15.0 }), 1.0);
        let empty = RoundedClip::new(RectF { width: 0.0, ..rect }, CornerRadii::default());
        assert!(empty.is_valid());
        assert_eq!(empty.coverage(PointF { x: -20.0, y: 15.0 }), 0.0);
        assert!(
            !RoundedClip {
                rect: RectF {
                    x: f32::NAN,
                    ..rect
                },
                ..square
            }
            .is_valid()
        );
    }
}

impl RoundedClip {
    pub fn new(rect: RectF, radii: CornerRadii) -> Self {
        let limit = (rect.width.min(rect.height) * 0.5).max(0.0);
        let radius = |r: f32| {
            if r.is_finite() {
                r.clamp(0.0, limit)
            } else {
                0.0
            }
        };
        Self {
            rect,
            radii: CornerRadii {
                top_left: radius(radii.top_left),
                top_right: radius(radii.top_right),
                bottom_right: radius(radii.bottom_right),
                bottom_left: radius(radii.bottom_left),
            },
        }
    }

    /// Matches the analytic box renderer's inner border contour, including zero-width borders.
    pub fn inset(self, border: Border) -> Self {
        let top = border.top.width.max(0.0);
        let right = border.right.width.max(0.0);
        let bottom = border.bottom.width.max(0.0);
        let left = border.left.width.max(0.0);
        Self::new(
            RectF {
                x: self.rect.x + left,
                y: self.rect.y + top,
                width: (self.rect.width - left - right).max(0.0),
                height: (self.rect.height - top - bottom).max(0.0),
            },
            CornerRadii {
                top_left: (self.radii.top_left - top.max(left)).max(0.0),
                top_right: (self.radii.top_right - top.max(right)).max(0.0),
                bottom_right: (self.radii.bottom_right - bottom.max(right)).max(0.0),
                bottom_left: (self.radii.bottom_left - bottom.max(left)).max(0.0),
            },
        )
    }

    pub fn is_valid(self) -> bool {
        [
            self.rect.x,
            self.rect.y,
            self.rect.width,
            self.rect.height,
            self.radii.top_left,
            self.radii.top_right,
            self.radii.bottom_right,
            self.radii.bottom_left,
        ]
        .into_iter()
        .all(f32::is_finite)
            && self.rect.width >= 0.0
            && self.rect.height >= 0.0
            && [
                self.radii.top_left,
                self.radii.top_right,
                self.radii.bottom_right,
                self.radii.bottom_left,
            ]
            .into_iter()
            .all(|r| r >= 0.0)
    }

    /// One-output-pixel analytic antialiasing, shared with the composite fragment shaders.
    pub fn coverage(self, point: PointF) -> f32 {
        if self.rect.width <= 0.0 || self.rect.height <= 0.0 {
            return 0.0;
        }
        let half_x = self.rect.width * 0.5;
        let half_y = self.rect.height * 0.5;
        let x = point.x - self.rect.x;
        let y = point.y - self.rect.y;
        let radius = if x < half_x {
            if y < half_y {
                self.radii.top_left
            } else {
                self.radii.bottom_left
            }
        } else if y < half_y {
            self.radii.top_right
        } else {
            self.radii.bottom_right
        };
        let radius = radius.min(half_x.min(half_y));
        let qx = (x - half_x).abs() - (half_x - radius);
        let qy = (y - half_y).abs() - (half_y - radius);
        let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
        (0.5 - distance).clamp(0.0, 1.0)
    }
}
