use kurbo::Point;

use crate::{Color, Lerp};

/// Shape of a [`Gradient`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientKind {
    /// Along the line `from` → `to`.
    Linear {
        /// Where the first stop is.
        from: Point,
        /// Where the last stop is.
        to: Point,
    },
    /// Circles around `center`, color at `radius` is the last stop.
    Radial {
        /// Where the first stop is.
        center: Point,
        /// Distance of the last stop from the center.
        radius: f64,
    },
}

/// A gradient in the node's local coordinates (origin at the node center).
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    /// Linear or radial, with its geometry.
    pub kind: GradientKind,
    /// `(offset in 0..=1, color)`, sorted by offset.
    pub stops: Vec<(f64, Color)>,
}

impl Gradient {
    /// Linear gradient from `from` to `to`; add colors with `stops`.
    pub fn linear(from: impl Into<Point>, to: impl Into<Point>) -> Self {
        Self {
            kind: GradientKind::Linear {
                from: from.into(),
                to: to.into(),
            },
            stops: Vec::new(),
        }
    }

    /// Radial gradient around `center`; add colors with `stops`.
    pub fn radial(center: impl Into<Point>, radius: f64) -> Self {
        Self {
            kind: GradientKind::Radial {
                center: center.into(),
                radius,
            },
            stops: Vec::new(),
        }
    }

    /// Adds a color stop at `offset` (0..=1).
    pub fn stop(mut self, offset: f64, color: Color) -> Self {
        self.stops.push((offset.clamp(0.0, 1.0), color));
        self.stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        self
    }

    /// Evenly spaced stops.
    pub fn stops(mut self, colors: impl IntoIterator<Item = Color>) -> Self {
        let colors: Vec<Color> = colors.into_iter().collect();
        let n = colors.len().max(2) - 1;
        self.stops = colors
            .into_iter()
            .enumerate()
            .map(|(i, c)| (i as f64 / n as f64, c))
            .collect();
        self
    }

    /// Multiplies every stop's alpha (node opacity).
    pub fn with_opacity(&self, opacity: f64) -> Self {
        Self {
            kind: self.kind,
            stops: self
                .stops
                .iter()
                .map(|(o, c)| (*o, c.with_alpha((f64::from(c.a) * opacity) as f32)))
                .collect(),
        }
    }

    /// Whether any stop is not fully transparent.
    pub fn is_visible(&self) -> bool {
        self.stops.iter().any(|(_, c)| c.a > 0.0)
    }
}

/// Same kind and stop count: points, offsets and colors interpolate.
/// Otherwise the gradient switches at the midpoint.
impl Lerp for Gradient {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        let kind = match (self.kind, to.kind) {
            (GradientKind::Linear { from: a, to: b }, GradientKind::Linear { from: c, to: d }) => {
                GradientKind::Linear {
                    from: Lerp::lerp(&a, &c, t),
                    to: Lerp::lerp(&b, &d, t),
                }
            }
            (
                GradientKind::Radial {
                    center: a,
                    radius: r1,
                },
                GradientKind::Radial {
                    center: b,
                    radius: r2,
                },
            ) => GradientKind::Radial {
                center: Lerp::lerp(&a, &b, t),
                radius: r1.lerp(&r2, t),
            },
            _ => return if t < 0.5 { self.clone() } else { to.clone() },
        };
        if self.stops.len() != to.stops.len() {
            return if t < 0.5 { self.clone() } else { to.clone() };
        }
        Self {
            kind,
            stops: self
                .stops
                .iter()
                .zip(&to.stops)
                .map(|((o1, c1), (o2, c2))| (o1.lerp(o2, t), c1.lerp(c2, t)))
                .collect(),
        }
    }
}

/// How a node's layer is composited onto what is below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendMode {
    #[default]
    /// Paints over (source-over).
    Normal,
    /// Darkens: multiplies colors.
    Multiply,
    /// Lightens: inverse of multiply.
    Screen,
    /// Multiply or screen, depending on the backdrop.
    Overlay,
    /// Keeps the darker color.
    Darken,
    /// Keeps the lighter color.
    Lighten,
    /// Brightens the backdrop.
    ColorDodge,
    /// Darkens the backdrop.
    ColorBurn,
    /// Multiply or screen, depending on the source.
    HardLight,
    /// A softer hard light.
    SoftLight,
    /// Absolute difference of the colors.
    Difference,
    /// Like difference, lower contrast.
    Exclusion,
    /// Source hue, backdrop saturation and luminosity.
    Hue,
    /// Source saturation.
    Saturation,
    /// Source hue and saturation.
    Color,
    /// Source luminosity.
    Luminosity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_lerp() {
        let a = Gradient::linear((0.0, 0.0), (10.0, 0.0)).stops([Color::BLACK, Color::WHITE]);
        let b = Gradient::linear((0.0, 0.0), (20.0, 0.0)).stops([Color::WHITE, Color::BLACK]);
        let m = a.lerp(&b, 0.5);
        assert_eq!(
            m.kind,
            GradientKind::Linear {
                from: Point::ZERO,
                to: Point::new(15.0, 0.0)
            }
        );
        assert_eq!(m.stops.len(), 2);
        let c = Gradient::radial((0.0, 0.0), 5.0).stop(0.0, Color::BLACK);
        assert_eq!(a.lerp(&c, 0.4), a);
        assert_eq!(a.lerp(&c, 0.6), c);
    }

    #[test]
    fn evenly_spaced_stops() {
        let g = Gradient::linear((0.0, 0.0), (1.0, 0.0)).stops([
            Color::BLACK,
            Color::WHITE,
            Color::BLACK,
        ]);
        assert_eq!(
            g.stops.iter().map(|s| s.0).collect::<Vec<_>>(),
            vec![0.0, 0.5, 1.0]
        );
    }
}
