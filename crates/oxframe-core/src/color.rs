use std::fmt;

/// sRGB color with straight (non-premultiplied) alpha, components in `0..=1`.
///
/// Interpolation ([`Lerp`](crate::Lerp)) happens in OKLab.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// Red, 0..=1 (sRGB-encoded).
    pub r: f32,
    /// Green, 0..=1.
    pub g: f32,
    /// Blue, 0..=1.
    pub b: f32,
    /// Alpha (straight, not premultiplied), 0..=1.
    pub a: f32,
}

impl Color {
    /// Fully transparent black.
    pub const TRANSPARENT: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);
    /// Opaque black.
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    /// Opaque white.
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);

    /// Opaque color from 0..=1 components.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Color from 0..=1 components with alpha.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Color from 0..=255 components.
    pub const fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Parses `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` (the `#` is optional).
    pub const fn from_hex(hex: &str) -> Option<Self> {
        let bytes = hex.as_bytes();
        let (start, len) = if !bytes.is_empty() && bytes[0] == b'#' {
            (1, bytes.len() - 1)
        } else {
            (0, bytes.len())
        };
        let short = len == 3 || len == 4;
        if !(short || len == 6 || len == 8) {
            return None;
        }
        let mut channels = [255u8; 4];
        let count = if len == 3 || len == 6 { 3 } else { 4 };
        let mut i = 0;
        while i < count {
            let v = if short {
                match hex_digit(bytes[start + i]) {
                    Some(d) => d * 17,
                    None => return None,
                }
            } else {
                match (
                    hex_digit(bytes[start + 2 * i]),
                    hex_digit(bytes[start + 2 * i + 1]),
                ) {
                    (Some(h), Some(l)) => h * 16 + l,
                    _ => return None,
                }
            };
            channels[i] = v;
            i += 1;
        }
        Some(Self::from_rgba8(
            channels[0],
            channels[1],
            channels[2],
            channels[3],
        ))
    }

    /// The same color with another alpha.
    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Components as 0..=255 bytes.
    pub fn to_rgba8(self) -> [u8; 4] {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [q(self.r), q(self.g), q(self.b), q(self.a)]
    }

    /// Converts to OKLab `[L, a, b]`, the space colors are interpolated in.
    pub fn to_oklab(self) -> [f32; 3] {
        let r = srgb_to_linear(self.r);
        let g = srgb_to_linear(self.g);
        let b = srgb_to_linear(self.b);
        let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        [
            0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        ]
    }

    /// Converts from OKLab `[L, a, b]`.
    pub fn from_oklab([ll, a, b]: [f32; 3], alpha: f32) -> Self {
        let l = (ll + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
        let m = (ll - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
        let s = (ll - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
        Self {
            r: linear_to_srgb(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s),
            g: linear_to_srgb(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s),
            b: linear_to_srgb(-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s),
            a: alpha,
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b, a] = self.to_rgba8();
        write!(f, "#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

const fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex() {
        assert_eq!(Color::from_hex("#fff"), Some(Color::WHITE));
        assert_eq!(Color::from_hex("000000"), Some(Color::BLACK));
        assert_eq!(
            Color::from_hex("#ff000080").unwrap().to_rgba8(),
            [255, 0, 0, 128]
        );
        assert_eq!(Color::from_hex("#ggg"), None);
        assert_eq!(Color::from_hex("#12345"), None);
    }

    #[test]
    fn oklab_roundtrip() {
        for hex in ["#e13238", "#00ff00", "#123456", "#ffffff", "#000000"] {
            let c = Color::from_hex(hex).unwrap();
            let back = Color::from_oklab(c.to_oklab(), c.a);
            assert_eq!(back.to_rgba8(), c.to_rgba8(), "{hex}");
        }
    }
}
